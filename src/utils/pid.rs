use std::process;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub start_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessProbe {
    pub exists: bool,
    pub start_token: Option<String>,
}

pub fn current_process_identity() -> Option<ProcessIdentity> {
    let probe = probe_process(process::id());
    probe
        .exists
        .then(|| probe.start_token)
        .flatten()
        .map(|start_token| ProcessIdentity { start_token })
}

pub fn probe_process(pid: u32) -> ProcessProbe {
    if pid == 0 {
        return ProcessProbe {
            exists: false,
            start_token: None,
        };
    }

    #[cfg(target_os = "linux")]
    {
        return probe_process_linux(pid);
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        return probe_process_unix(pid);
    }

    #[cfg(windows)]
    {
        return probe_process_windows(pid);
    }

    #[allow(unreachable_code)]
    ProcessProbe {
        exists: true,
        start_token: None,
    }
}

#[cfg(target_os = "linux")]
fn probe_process_linux(pid: u32) -> ProcessProbe {
    use std::fs;
    use std::path::Path;

    let proc_dir = Path::new("/proc").join(pid.to_string());
    if !proc_dir.exists() {
        return ProcessProbe {
            exists: false,
            start_token: None,
        };
    }

    let stat_path = proc_dir.join("stat");
    let start_token = fs::read_to_string(stat_path)
        .ok()
        .and_then(|raw| parse_linux_start_time_ticks(&raw))
        .map(|ticks| ticks.to_string());

    ProcessProbe {
        exists: true,
        start_token,
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_start_time_ticks(raw_stat: &str) -> Option<u64> {
    // /proc/<pid>/stat format includes "(comm)" which may contain spaces.
    // Field 22 (1-based) is starttime; after stripping "(comm) ", it is index 19.
    let close_paren = raw_stat.rfind(") ")?;
    let rest = &raw_stat[(close_paren + 2)..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    fields.get(19)?.parse::<u64>().ok()
}

#[cfg(all(unix, not(target_os = "linux")))]
fn probe_process_unix(pid: u32) -> ProcessProbe {
    use std::process::Command;

    let exists = kill_zero_exists(pid);
    if !exists {
        return ProcessProbe {
            exists: false,
            start_token: None,
        };
    }

    let start_token = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|stdout| stdout.trim().to_string())
        .filter(|token| !token.is_empty());

    ProcessProbe {
        exists: true,
        start_token,
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn kill_zero_exists(pid: u32) -> bool {
    use std::process::Command;

    match Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            if stderr.contains("no such process") {
                false
            } else {
                // Conservative fallback on permission or unknown errors.
                true
            }
        }
        Err(_) => true,
    }
}

#[cfg(windows)]
fn probe_process_windows(pid: u32) -> ProcessProbe {
    use std::mem::MaybeUninit;
    use std::ptr;
    use winapi::shared::minwindef::{DWORD, FALSE};
    use winapi::shared::winerror::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::minwinbase::FILETIME;
    use winapi::um::processthreadsapi::{GetExitCodeProcess, GetProcessTimes, OpenProcess};
    use winapi::um::winbase::STILL_ACTIVE;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid as DWORD);
        if handle.is_null() {
            let err = GetLastError();
            return match err {
                ERROR_INVALID_PARAMETER => ProcessProbe {
                    exists: false,
                    start_token: None,
                },
                ERROR_ACCESS_DENIED => ProcessProbe {
                    exists: true,
                    start_token: None,
                },
                _ => ProcessProbe {
                    exists: true,
                    start_token: None,
                },
            };
        }

        let mut exit_code: DWORD = 0;
        if GetExitCodeProcess(handle, &mut exit_code as *mut DWORD) == 0 {
            CloseHandle(handle);
            return ProcessProbe {
                exists: true,
                start_token: None,
            };
        }

        if exit_code != STILL_ACTIVE {
            CloseHandle(handle);
            return ProcessProbe {
                exists: false,
                start_token: None,
            };
        }

        let mut creation = MaybeUninit::<FILETIME>::uninit();
        let mut exit = MaybeUninit::<FILETIME>::uninit();
        let mut kernel = MaybeUninit::<FILETIME>::uninit();
        let mut user = MaybeUninit::<FILETIME>::uninit();

        let start_token = if GetProcessTimes(
            handle,
            creation.as_mut_ptr(),
            exit.as_mut_ptr(),
            kernel.as_mut_ptr(),
            user.as_mut_ptr(),
        ) != 0
        {
            let creation = creation.assume_init();
            let ticks =
                ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
            Some(ticks.to_string())
        } else {
            None
        };

        CloseHandle(handle);
        ProcessProbe {
            exists: true,
            start_token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{current_process_identity, probe_process};

    #[test]
    fn probe_current_process_reports_exists() {
        let probe = probe_process(std::process::id());
        assert!(probe.exists);
    }

    #[test]
    fn probe_pid_zero_reports_not_exists() {
        let probe = probe_process(0);
        assert!(!probe.exists);
    }

    #[test]
    fn current_identity_token_is_non_empty_when_available() {
        if let Some(identity) = current_process_identity() {
            assert!(!identity.start_token.trim().is_empty());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_linux_start_time_from_stat_line() {
        let raw = "1234 (process name) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 212121 20";
        let parsed = super::parse_linux_start_time_ticks(raw);
        assert_eq!(parsed, Some(212121));
    }
}
