//! The Recycle Bin / Trash as one item: its size and a real "empty". Scans never list it
//! (`$Recycle.Bin` is a protected tree) because each file inside is already deleted by the user.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BinStatus {
    pub bytes: u64,
    pub items: u64,
}

#[cfg(windows)]
pub fn status() -> Result<BinStatus, String> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: `info` is a valid, correctly sized SHQUERYRBINFO; a null root queries every drive.
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr < 0 {
        return Err(format!("SHQueryRecycleBin failed (0x{:08X})", hr as u32));
    }
    Ok(BinStatus { bytes: info.i64Size.max(0) as u64, items: info.i64NumItems.max(0) as u64 })
}

#[cfg(windows)]
pub fn empty() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    if status()?.items == 0 {
        return Ok(());
    }
    // SAFETY: null window + null root; flags are valid SHERB_* constants.
    let hr = unsafe {
        SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
    };
    if hr < 0 {
        return Err(format!("SHEmptyRecycleBin failed (0x{:08X})", hr as u32));
    }
    Ok(())
}

#[cfg(not(windows))]
fn trash_dir() -> PathBuf {
    let home = crate::config::home_dir();
    if cfg!(target_os = "macos") {
        home.join(".Trash")
    } else {
        dirs::data_dir().unwrap_or_else(|| home.join(".local/share")).join("Trash").join("files")
    }
}

#[cfg(not(windows))]
pub fn status() -> Result<BinStatus, String> {
    let dir = trash_dir();
    if !dir.is_dir() {
        return Ok(BinStatus::default());
    }
    let rd = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut st = BinStatus::default();
    for e in rd.flatten() {
        st.items += 1;
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() && !t.is_symlink() => {
                st.bytes += crate::core::scanner::dir_stats(&p, &std::sync::atomic::AtomicBool::new(false)).size
            }
            Ok(t) if !t.is_symlink() => st.bytes += e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => {}
        }
    }
    Ok(st)
}

#[cfg(not(windows))]
pub fn empty() -> Result<(), String> {
    if cfg!(target_os = "macos") {
        // Finder empties it properly (also drives' trashes); reading ~/.Trash needs Full Disk Access.
        let out = std::process::Command::new("osascript")
            .args(["-e", "tell application \"Finder\" to empty the trash"])
            .output()
            .map_err(|e| e.to_string())?;
        return if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) };
    }
    let files = trash_dir();
    let info = files.with_file_name("info");
    for d in [files, info] {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                let r = if e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false) {
                    std::fs::remove_dir_all(&p)
                } else {
                    std::fs::remove_file(&p)
                };
                r.map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
    }
    Ok(())
}

/// Where the bin lives, for display only.
pub fn location() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"$Recycle.Bin")
    } else if cfg!(target_os = "macos") {
        crate::config::home_dir().join(".Trash")
    } else {
        dirs::data_dir().unwrap_or_default().join("Trash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_readable() {
        // Never call `empty()` here: it would empty the real bin of whoever runs the tests.
        assert!(status().is_ok());
    }
}
