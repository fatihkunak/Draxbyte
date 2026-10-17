//! Never-delete whitelist, lock detection and the last check before a delete.

use crate::config::home_dir;
use crate::core::scanner::dir_stats;
use crate::models::{Category, ScanItem};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

/// Lower-cased, `/`-separated, no trailing slash. Case-folding is done on every OS
/// on purpose: refusing too much is the safe failure.
pub fn norm(p: &Path) -> String {
    let mut s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    if let Some(stripped) = s.strip_prefix("//?/") {
        s = stripped.to_string();
    }
    while s.len() > 1 && s.ends_with('/') && !s.ends_with(":/") {
        s.pop();
    }
    s
}

fn under(s: &str, prefix: &str) -> bool {
    s == prefix || s.starts_with(&format!("{}/", prefix.trim_end_matches('/')))
}

struct Lists {
    /// The path and everything below it is protected.
    trees: Vec<String>,
    /// Only this exact path is protected (the contents may be cleaned).
    exact: Vec<String>,
    /// Re-allowed paths that sit inside a protected tree (e.g. `C:\Windows\Temp`).
    exceptions: Vec<String>,
}

fn lists() -> &'static Lists {
    static L: OnceLock<Lists> = OnceLock::new();
    L.get_or_init(|| {
        let mut trees: Vec<String> = Vec::new();
        let mut exceptions: Vec<String> = Vec::new();
        let mut exact: Vec<String> = Vec::new();

        if cfg!(windows) {
            let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            let drive = sysroot.chars().take(2).collect::<String>();
            trees.push(norm(Path::new(&sysroot)));
            exceptions.push(norm(&Path::new(&sysroot).join("Temp")));
            for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                if let Ok(v) = std::env::var(var) {
                    trees.push(norm(Path::new(&v)));
                }
            }
            for d in [
                "Recovery",
                "System Volume Information",
                "$Recycle.Bin",
                "Boot",
                "EFI",
            ] {
                trees.push(norm(Path::new(&format!("{}\\{}", drive, d))));
            }
        } else {
            for p in [
                "/bin",
                "/boot",
                "/dev",
                "/etc",
                "/lib",
                "/lib32",
                "/lib64",
                "/proc",
                "/run",
                "/sbin",
                "/sys",
                "/usr",
                "/System",
                "/Library",
                "/Applications",
                "/private/etc",
                "/var/lib",
                "/var/db",
            ] {
                trees.push(p.to_lowercase());
            }
        }

        let home = home_dir();
        // Secrets: never offered, wherever they sit under home.
        for d in [".ssh", ".gnupg", ".aws", ".kube"] {
            trees.push(norm(&home.join(d)));
        }
        // Our own data (config, caches, trash log).
        trees.push(norm(&crate::config::data_dir()));

        exact.push(norm(&home));
        for d in [
            "Documents",
            "Desktop",
            "Downloads",
            "Pictures",
            "Videos",
            "Music",
            "AppData",
            "Library",
        ] {
            exact.push(norm(&home.join(d)));
        }
        Lists {
            trees,
            exact,
            exceptions,
        }
    })
}

fn is_root(p: &Path) -> bool {
    p.parent().is_none()
        || (cfg!(windows) && {
            let s = norm(p);
            s.len() == 2 && s.ends_with(':') || s.len() == 3 && s.ends_with(":/")
        })
}

/// The path is inside a system/secret tree (or is a filesystem root).
/// The scanner prunes these; they are never listed.
pub fn is_protected(path: &Path) -> bool {
    if is_root(path) {
        return true;
    }
    let s = norm(path);
    let l = lists();
    if l.exceptions.iter().any(|e| under(&s, e)) {
        return false;
    }
    l.trees.iter().any(|t| under(&s, t))
}

/// Stricter, for things that could be deleted: also the home dir and the
/// standard user folders themselves.
pub fn is_protected_item(path: &Path) -> bool {
    is_protected(path) || {
        let s = norm(path);
        lists().exact.contains(&s)
    }
}

/// True when another process holds the file open exclusively. Only Windows
/// enforces mandatory locks; elsewhere deleting an open file is harmless.
#[cfg(windows)]
pub fn is_locked(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
    {
        Ok(_) => false,
        // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33)),
    }
}

#[cfg(not(windows))]
pub fn is_locked(_path: &Path) -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Protected,
    RedCategory,
    Missing,
    Symlink,
    Locked,
    Changed,
}

impl Skip {
    pub fn message(&self) -> &'static str {
        match self {
            Skip::Protected => "korumalı yol",
            Skip::RedCategory => "kırmızı kategori (silinemez)",
            Skip::Missing => "artık yok",
            Skip::Symlink => "sembolik bağ (takip edilmez)",
            Skip::Locked => "dosya kilitli / kullanımda",
            Skip::Changed => "taramadan sonra değişmiş",
        }
    }
}

/// Last look right before touching an item: re-stat everything, trust nothing
/// from the scan.
pub fn pre_delete_check(item: &ScanItem) -> Result<(), Skip> {
    if item.category == Category::Red {
        return Err(Skip::RedCategory);
    }
    if is_protected_item(&item.path) {
        return Err(Skip::Protected);
    }
    let meta = std::fs::symlink_metadata(&item.path).map_err(|_| Skip::Missing)?;
    if meta.file_type().is_symlink() {
        return Err(Skip::Symlink);
    }
    if item.is_dir {
        if !meta.is_dir() {
            return Err(Skip::Changed);
        }
        let st = dir_stats(&item.path, &AtomicBool::new(false));
        if st.newest > item.mtime {
            return Err(Skip::Changed);
        }
    } else {
        if !meta.is_file() {
            return Err(Skip::Changed);
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if mtime != item.mtime || meta.len() != item.size {
            return Err(Skip::Changed);
        }
        if is_locked(&item.path) {
            return Err(Skip::Locked);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_protected() {
        let root = if cfg!(windows) {
            Path::new("C:\\")
        } else {
            Path::new("/")
        };
        assert!(is_protected(root));
    }

    #[cfg(windows)]
    #[test]
    fn windows_system_is_protected_but_temp_is_not() {
        assert!(is_protected(Path::new(r"C:\Windows\System32\kernel32.dll")));
        assert!(is_protected(Path::new(r"c:\WINDOWS")));
        assert!(!is_protected(Path::new(r"C:\Windows\Temp\x.tmp")));
        assert!(is_protected(Path::new(r"C:\Program Files\App\app.exe")));
    }

    #[cfg(unix)]
    #[test]
    fn unix_system_is_protected() {
        assert!(is_protected(Path::new("/usr/bin/ls")));
        assert!(is_protected(Path::new("/etc/passwd")));
        assert!(!is_protected(Path::new("/tmp/x.tmp")));
    }

    #[test]
    fn secrets_and_home_are_protected() {
        let home = home_dir();
        assert!(is_protected(&home.join(".ssh").join("id_ed25519")));
        assert!(is_protected_item(&home));
        assert!(is_protected_item(&home.join("Documents")));
        assert!(!is_protected_item(&home.join("Documents").join("big.iso")));
    }

    #[test]
    fn prefix_match_is_component_wise() {
        // "windows-old" must not match the "windows" tree.
        assert!(!under("c:/windows-old/x", "c:/windows"));
        assert!(under("c:/windows/x", "c:/windows"));
    }

    fn file_item(path: &Path, size: u64, mtime: i64) -> ScanItem {
        ScanItem {
            path: path.to_path_buf(),
            size,
            mtime,
            is_dir: false,
            category: Category::Green,
            rule: "t".into(),
            reason: "t".into(),
            unknown: false,
        }
    }

    fn mtime_of(p: &Path) -> i64 {
        std::fs::metadata(p)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn pre_delete_check_detects_change_missing_and_red() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.tmp");
        std::fs::write(&f, b"hello").unwrap();
        let ok = file_item(&f, 5, mtime_of(&f));
        assert_eq!(pre_delete_check(&ok), Ok(()));

        let stale = file_item(&f, 6, mtime_of(&f));
        assert_eq!(pre_delete_check(&stale), Err(Skip::Changed));

        let mut red = ok.clone();
        red.category = Category::Red;
        assert_eq!(pre_delete_check(&red), Err(Skip::RedCategory));

        let gone = file_item(&dir.path().join("zzz"), 1, 0);
        assert_eq!(pre_delete_check(&gone), Err(Skip::Missing));
    }

    #[cfg(windows)]
    #[test]
    fn exclusively_opened_file_is_locked() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("held.tmp");
        std::fs::write(&f, b"x").unwrap();
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&f)
            .unwrap();
        assert!(is_locked(&f));
    }
}
