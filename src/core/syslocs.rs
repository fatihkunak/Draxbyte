//! Places a scan used to skip: system temp/log/cache folders and every fixed drive.
//! Pure path logic lives here so it can be tested on any OS; only the drive list needs the OS.

use crate::core::safety::is_protected;
use std::path::{Component, Path, PathBuf};

/// System folders that are safe to look into even though they sit inside a protected tree
/// (`C:\Windows`). Each entry is also an exception in `safety::lists`.
#[cfg(windows)]
pub fn windows_system_dirs() -> Vec<PathBuf> {
    let sysroot = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
    let pd = PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()));
    let drive = sysroot
        .components()
        .next()
        .map(|c| PathBuf::from(c.as_os_str()).join("\\"))
        .unwrap_or_else(|| PathBuf::from(r"C:\"));
    vec![
        sysroot.join("Temp"),
        sysroot.join("Logs").join("CBS"),
        sysroot.join("Logs").join("DISM"),
        sysroot.join("SoftwareDistribution").join("Download"),
        sysroot.join("Prefetch"),
        sysroot.join("Minidump"),
        sysroot.join("LiveKernelReports"),
        sysroot.join("Installer").join("$PatchCache$"),
        sysroot.join("ServiceProfiles").join("LocalService").join("AppData").join("Local").join("Temp"),
        pd.join("Package Cache"),
        pd.join("Microsoft").join("Windows").join("WER"),
        drive.join("Windows.old"),
    ]
}

#[cfg(not(windows))]
pub fn windows_system_dirs() -> Vec<PathBuf> {
    Vec::new()
}

/// Extra scan roots for the system profile (existing folders only).
pub fn system_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = if cfg!(windows) {
        windows_system_dirs()
    } else if cfg!(target_os = "macos") {
        ["/Library/Caches", "/Library/Logs", "/Library/Updates", "/private/var/log", "/Users/Shared"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        ["/var/cache", "/var/log"].iter().map(PathBuf::from).collect()
    };
    v.retain(|p| p.is_dir());
    v
}

/// Mount points of internal (non-removable) disks. Windows: `C:\`, `D:\`…; elsewhere only
/// extra data disks (the system volume itself is covered by the home scan and the system roots).
pub fn fixed_drive_roots() -> Vec<PathBuf> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut v: Vec<PathBuf> = disks
        .list()
        .iter()
        .filter(|d| !d.is_removable() && d.total_space() > 0)
        .map(|d| d.mount_point().to_path_buf())
        .filter(|m| {
            if cfg!(windows) {
                true
            } else {
                let s = m.to_string_lossy();
                s != "/" && !s.starts_with("/System/") && !s.starts_with("/private/") && !s.starts_with("/boot")
                    && !s.starts_with("/dev") && !s.starts_with("/run") && !s.starts_with("/sys") && !s.starts_with("/proc")
            }
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

fn starts_with_ci(child: &Path, parent: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        p.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let (c, p) = (norm(child), norm(parent));
    c.len() >= p.len() && c[..p.len()] == p[..]
}

fn same_path(a: &Path, b: &Path) -> bool {
    starts_with_ci(a, b) && starts_with_ci(b, a)
}

/// True when walking down from `outer` would never reach `inner`: a protected folder
/// (e.g. `C:\Windows`) lies strictly between them.
fn blocked_between(outer: &Path, inner: &Path) -> bool {
    let mut cur = inner.parent();
    while let Some(p) = cur {
        if same_path(p, outer) || !starts_with_ci(p, outer) {
            return false;
        }
        if is_protected(p) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Drops roots that another root already walks over, so nothing is scanned twice.
/// A nested root is kept when a protected folder lies in between (the outer walk skips it).
pub fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (i, r) in roots.iter().enumerate() {
        let covered = roots.iter().enumerate().any(|(j, o)| {
            if i == j || !starts_with_ci(r, o) {
                return false;
            }
            if same_path(r, o) {
                return j < i; // same folder spelled twice: keep the first
            }
            !blocked_between(o, r)
        });
        if !covered {
            out.push(r.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn nested_root_is_dropped() {
        let r = dedupe_roots(vec![p("/data"), p("/data/a/b"), p("/other")]);
        assert_eq!(r, vec![p("/data"), p("/other")]);
    }

    #[test]
    fn duplicates_and_case_variants_collapse() {
        let r = dedupe_roots(vec![p("/Data"), p("/data"), p("/DATA/x")]);
        assert_eq!(r, vec![p("/Data")]);
    }

    #[cfg(unix)]
    #[test]
    fn root_inside_protected_folder_is_kept() {
        // "/" walks everything except protected trees, so /usr/share/x is never reached from it.
        let r = dedupe_roots(vec![p("/"), p("/usr/share/x"), p("/home/u")]);
        assert!(r.contains(&p("/usr/share/x")));
        assert!(!r.contains(&p("/home/u")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_temp_kept_but_programdata_dropped_under_drive_root() {
        let r = dedupe_roots(vec![
            p(r"C:\"),
            p(r"C:\Windows\Temp"),
            p(r"C:\ProgramData\Package Cache"),
            p(r"C:\Users\x"),
        ]);
        assert!(r.contains(&p(r"C:\")));
        assert!(r.contains(&p(r"C:\Windows\Temp")));
        assert!(!r.contains(&p(r"C:\ProgramData\Package Cache")));
        assert!(!r.contains(&p(r"C:\Users\x")));
    }

    #[test]
    fn system_roots_only_lists_existing_dirs() {
        for r in system_roots() {
            assert!(r.is_dir(), "{} should exist", r.display());
        }
    }
}
