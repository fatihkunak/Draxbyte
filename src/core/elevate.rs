//! Administrator rights: are we elevated, and restart elevated (one UAC prompt).
//! Draxbyte runs as a normal user; only removing system-wide programs, services, system folders
//! and the like need more.

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: standard token query on the current process; the handle is closed before returning.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() == 0 }
}

/// Starts this program again with administrator rights (the user sees the system's UAC prompt)
/// and returns once it was launched; the caller then closes itself. `Err` = the user declined.
#[cfg(windows)]
pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut c = std::process::Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Process -FilePath $env:DRAXBYTE_EXE -Verb RunAs",
    ])
    .env("DRAXBYTE_EXE", &exe);
    crate::core::memory::hide_window(&mut c);
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(windows))]
pub fn relaunch_elevated() -> Result<(), String> {
    Err(crate::i18n::t(
        "Bu platformda yönetici izni, işlem sırasında ayrıca istenir.",
        "On this platform administrator rights are requested per action.",
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn elevation_check_runs() {
        // Value depends on how the tests are started; it must simply not crash.
        let _ = super::is_elevated();
    }
}
