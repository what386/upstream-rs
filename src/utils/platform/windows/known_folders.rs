use anyhow::{Result, anyhow};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf, ptr, slice};
use winapi::{
    shared::guiddef::GUID,
    um::{
        combaseapi::CoTaskMemFree,
        knownfolders::FOLDERID_Programs,
        shlobj::{KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
    },
};

pub fn programs_dir() -> Result<PathBuf> {
    known_folder(&FOLDERID_Programs)
}

fn known_folder(id: &GUID) -> Result<PathBuf> {
    let mut raw_path = ptr::null_mut();
    // SAFETY: id and raw_path are valid pointers for the duration of the call.
    // A null token selects the current user. DONT_VERIFY permits the caller to
    // create the resolved directory later without assuming its default location.
    let status =
        unsafe { SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, ptr::null_mut(), &mut raw_path) };

    let result = if status < 0 || raw_path.is_null() {
        Err(anyhow!(
            "Failed to resolve Windows known folder (HRESULT {status:#010x})"
        ))
    } else {
        // SAFETY: On success Windows returns a null-terminated UTF-16 string
        // that remains valid until CoTaskMemFree below.
        let path = unsafe {
            let mut length = 0;
            while *raw_path.add(length) != 0 {
                length += 1;
            }

            OsString::from_wide(slice::from_raw_parts(raw_path, length))
        };

        Ok(PathBuf::from(path))
    };

    // SAFETY: SHGetKnownFolderPath allocates with the COM task allocator. Its
    // output must be freed on both success and failure; freeing null is valid.
    unsafe { CoTaskMemFree(raw_path.cast()) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_folder_resolves_for_current_user() {
        assert!(
            programs_dir()
                .expect("resolve Programs known folder")
                .is_absolute()
        );
    }

    #[test]
    fn unknown_folder_returns_an_error() {
        let unknown = GUID {
            Data1: 0,
            Data2: 0,
            Data3: 0,
            Data4: [0; 8],
        };

        assert!(known_folder(&unknown).is_err());
    }
}
