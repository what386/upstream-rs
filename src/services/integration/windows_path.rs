use anyhow::{Context, Result, bail};
use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, ptr};
use winapi::{
    shared::minwindef::FALSE,
    um::{
        handleapi::CloseHandle,
        synchapi::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        winbase::{INFINITE, WAIT_FAILED},
        winnt::HANDLE,
        winuser::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE},
    },
};
use winreg::{
    RegKey, RegValue,
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ},
};

const MUTEX_NAME: &str = r#"Local\UpstreamPathMutation"#;
const MAX_CONCURRENT_RETRIES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRegistryType {
    String,
    ExpandString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsPathValue {
    pub value: String,
    pub registry_type: PathRegistryType,
}

pub struct WindowsPathManager;

impl WindowsPathManager {
    pub fn read() -> Result<Option<WindowsPathValue>> {
        let key = environment_key(KEY_READ)?;
        match key.get_raw_value("Path") {
            Ok(value) => decode_path_value(&value).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("Failed to read the user PATH registry value"),
        }
    }

    pub fn ensure_present(path: &str) -> Result<bool> {
        Self::mutate(|entries| {
            let expected = normalize(path);
            if entries.iter().any(|entry| normalize(entry) == expected) {
                return false;
            }
            entries.insert(0, path.to_string());
            true
        })
    }

    pub fn remove(path: &str) -> Result<bool> {
        Self::mutate(|entries| {
            let expected = normalize(path);
            let before = entries.len();
            entries.retain(|entry| normalize(entry) != expected);
            entries.len() != before
        })
    }

    pub fn replace(old_path: &str, new_path: &str) -> Result<bool> {
        Self::mutate(|entries| {
            let old = normalize(old_path);
            let new = normalize(new_path);
            let before = entries.clone();
            entries.retain(|entry| {
                let entry = normalize(entry);
                entry != old && entry != new
            });
            entries.insert(0, new_path.to_string());
            *entries != before
        })
    }

    pub fn contains(value: &str, path: &str) -> bool {
        let expected = normalize(path);
        value.split(';').any(|entry| normalize(entry) == expected)
    }

    fn mutate(mut update: impl FnMut(&mut Vec<String>) -> bool) -> Result<bool> {
        let _mutex = NamedMutex::acquire()?;
        let key = environment_key(KEY_READ | KEY_WRITE)?;
        for _ in 0..MAX_CONCURRENT_RETRIES {
            let raw_before = match key.get_raw_value("Path") {
                Ok(raw) => Some(raw),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(error).context("Failed to read the user PATH registry value");
                }
            };
            let decoded = match raw_before.as_ref() {
                Some(raw) => decode_path_value(raw)?,
                None => WindowsPathValue {
                    value: String::new(),
                    registry_type: PathRegistryType::ExpandString,
                },
            };

            let mut entries = decoded
                .value
                .split(';')
                .filter(|entry| !entry.trim().is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !update(&mut entries) {
                return Ok(false);
            }

            let raw_now = match key.get_raw_value("Path") {
                Ok(raw) => Some(raw),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(error).context("Failed to re-read the user PATH registry value");
                }
            };
            if raw_now != raw_before {
                continue;
            }

            let new_value = WindowsPathValue {
                value: entries.join(";"),
                registry_type: decoded.registry_type,
            };
            let encoded = encode_path_value(&new_value);
            key.set_raw_value("Path", &encoded)
                .context("Failed to write the user PATH registry value")?;
            let verified = key
                .get_raw_value("Path")
                .context("Failed to verify the user PATH registry value")?;
            if verified != encoded {
                continue;
            }
            broadcast_environment_change();
            return Ok(true);
        }
        bail!("The user PATH changed concurrently; retry the command")
    }
}

fn environment_key(access: u32) -> Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", access)
        .context("Failed to open HKEY_CURRENT_USER\\Environment")
}

fn normalize(path: &str) -> String {
    let mut normalized = path.trim().trim_matches('"').replace('/', "\\");
    while normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized.to_ascii_lowercase()
}

fn decode_path_value(raw: &RegValue<'_>) -> Result<WindowsPathValue> {
    let registry_type = match &raw.vtype {
        REG_SZ => PathRegistryType::String,
        REG_EXPAND_SZ => PathRegistryType::ExpandString,
        other => bail!("User PATH has unsupported registry type {other:?}"),
    };
    if raw.bytes.len() % 2 != 0 {
        bail!("User PATH registry data contains an incomplete UTF-16 code unit");
    }
    let mut words = raw
        .bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    while words.last() == Some(&0) {
        words.pop();
    }
    let value =
        String::from_utf16(&words).context("User PATH registry data is not valid UTF-16")?;
    Ok(WindowsPathValue {
        value,
        registry_type,
    })
}

fn encode_path_value(value: &WindowsPathValue) -> RegValue<'_> {
    let mut words = value.value.encode_utf16().collect::<Vec<_>>();
    words.push(0);
    RegValue {
        bytes: words
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
        vtype: match value.registry_type {
            PathRegistryType::String => REG_SZ,
            PathRegistryType::ExpandString => REG_EXPAND_SZ,
        },
    }
}

fn broadcast_environment_change() {
    let environment: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            environment.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            ptr::null_mut(),
        );
    }
}

struct NamedMutex(HANDLE);

impl NamedMutex {
    fn acquire() -> Result<Self> {
        let name = OsStr::new(MUTEX_NAME)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(ptr::null_mut(), FALSE, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error()).context("Failed to create PATH writer mutex");
        }
        if unsafe { WaitForSingleObject(handle, INFINITE) } == WAIT_FAILED {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error()).context("Failed to acquire PATH writer mutex");
        }
        Ok(Self(handle))
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_path_roundtrips_both_supported_registry_types() {
        for registry_type in [PathRegistryType::String, PathRegistryType::ExpandString] {
            let value = WindowsPathValue {
                value: r#"%USERPROFILE%\bin;C:\Tools"#.into(),
                registry_type,
            };
            assert_eq!(
                decode_path_value(&encode_path_value(&value)).unwrap(),
                value
            );
        }
    }

    #[test]
    fn path_comparison_is_case_insensitive_and_separator_agnostic() {
        assert!(WindowsPathManager::contains(
            r#"C:\Other;C:\Users\Me\.upstream\state\symlinks\"#,
            r#"c:/users/me/.UPSTREAM/state/symlinks"#
        ));
    }

    #[test]
    fn rejects_unreadable_registry_data_instead_of_decoding_it_as_empty() {
        let error = decode_path_value(&RegValue {
            bytes: vec![0].into(),
            vtype: REG_EXPAND_SZ,
        })
        .unwrap_err();
        assert!(error.to_string().contains("incomplete UTF-16"));
    }
}
