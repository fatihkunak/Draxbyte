//! Programs that start with Windows: the `Run` registry keys and the Startup folders.
//!
//! Disabling uses the same switch Task Manager uses (`...\Explorer\StartupApproved\...`), so
//! nothing is deleted and the entry can be turned back on. Machine-wide entries need admin rights.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Machine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A value under a `Run` key: (scope, 32-bit view).
    RunKey(Scope, bool),
    /// A shortcut/program in a Startup folder.
    Folder(Scope),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    pub source: Source,
    pub enabled: bool,
}

/// `StartupApproved` data: bit 0 of the first byte set = disabled; missing/short data = enabled.
pub fn approved_is_enabled(data: &[u8]) -> bool {
    data.first().map(|b| b & 1 == 0).unwrap_or(true)
}

/// The 12 bytes Task Manager writes: state DWORD + FILETIME of the change (zero when enabling).
pub fn approved_bytes(enabled: bool, filetime: u64) -> Vec<u8> {
    let mut v = vec![if enabled { 2 } else { 3 }, 0, 0, 0];
    v.extend_from_slice(&(if enabled { 0 } else { filetime }).to_le_bytes());
    v
}

/// Windows FILETIME (100 ns ticks since 1601) for "now".
pub fn filetime_now() -> u64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (unix + 11_644_473_600) * 10_000_000
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    fn hive(scope: Scope) -> RegKey {
        RegKey::predef(if scope == Scope::User { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE })
    }

    fn approved_sub(source: &Source) -> String {
        match source {
            Source::RunKey(_, false) => format!(r"{APPROVED}\Run"),
            Source::RunKey(_, true) => format!(r"{APPROVED}\Run32"),
            Source::Folder(_) => format!(r"{APPROVED}\StartupFolder"),
        }
    }

    fn scope_of(source: &Source) -> Scope {
        match source {
            Source::RunKey(s, _) | Source::Folder(s) => *s,
        }
    }

    fn is_enabled(root: &RegKey, source: &Source, name: &str) -> bool {
        root.open_subkey(approved_sub(source))
            .and_then(|k| k.get_raw_value(name))
            .map(|v| approved_is_enabled(&v.bytes))
            .unwrap_or(true)
    }

    fn startup_dirs() -> Vec<(Scope, std::path::PathBuf)> {
        let mut v = Vec::new();
        if let Ok(a) = std::env::var("APPDATA") {
            v.push((Scope::User, std::path::PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Startup")));
        }
        if let Ok(p) = std::env::var("ProgramData") {
            v.push((Scope::Machine, std::path::PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs\StartUp")));
        }
        v
    }

    pub fn list() -> Vec<StartupEntry> {
        let mut out = Vec::new();
        for (scope, path, wow) in [
            (Scope::User, RUN, false),
            (Scope::Machine, RUN, false),
            (Scope::Machine, RUN32, true),
        ] {
            let root = hive(scope);
            let Ok(key) = root.open_subkey(path) else { continue };
            let source = Source::RunKey(scope, wow);
            for (name, val) in key.enum_values().flatten() {
                let command = match val.vtype {
                    REG_SZ | REG_EXPAND_SZ => String::from_utf16_lossy(
                        &val.bytes
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .take_while(|&c| c != 0)
                            .collect::<Vec<u16>>(),
                    ),
                    _ => String::new(),
                };
                let enabled = is_enabled(&root, &source, &name);
                out.push(StartupEntry { name, command, source: source.clone(), enabled });
            }
        }
        for (scope, dir) in startup_dirs() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let root = hive(scope);
            let source = Source::Folder(scope);
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                let enabled = is_enabled(&root, &source, &name);
                out.push(StartupEntry { name, command: e.path().to_string_lossy().into_owned(), source: source.clone(), enabled });
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// Writes the approval flag under `root` (testable with a scratch key).
    pub fn write_approved(root: &RegKey, sub: &str, name: &str, enabled: bool) -> Result<(), String> {
        let (key, _) = root.create_subkey_with_flags(sub, KEY_SET_VALUE | KEY_QUERY_VALUE).map_err(|e| e.to_string())?;
        let raw = RegValue { bytes: approved_bytes(enabled, filetime_now()).into(), vtype: REG_BINARY };
        key.set_raw_value(name, &raw).map_err(|e| e.to_string())
    }

    pub fn set_enabled(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        let scope = scope_of(&entry.source);
        write_approved(&hive(scope), &approved_sub(&entry.source), &entry.name, enabled).map_err(|e| {
            if scope == Scope::Machine {
                format!("{e} ({})", crate::i18n::t("yönetici olarak aç", "run as administrator"))
            } else {
                e
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn disable_then_enable_roundtrips_in_a_scratch_key() {
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let base = format!(r"Software\DraxbyteTest-startup-{}", std::process::id());
            let sub = format!(r"{base}\StartupApproved\Run");
            write_approved(&root, &sub, "Foo", false).unwrap();
            let raw = root.open_subkey(&sub).unwrap().get_raw_value("Foo").unwrap();
            assert_eq!(raw.bytes.len(), 12);
            assert!(!approved_is_enabled(&raw.bytes));
            write_approved(&root, &sub, "Foo", true).unwrap();
            let raw = root.open_subkey(&sub).unwrap().get_raw_value("Foo").unwrap();
            assert!(approved_is_enabled(&raw.bytes));
            root.delete_subkey_all(&base).unwrap();
        }

        #[test]
        fn list_runs_on_this_machine() {
            // Real registry, read-only: must not panic, every entry has a name.
            for e in list() {
                assert!(!e.name.is_empty());
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{list, set_enabled};

#[cfg(not(windows))]
pub fn list() -> Vec<StartupEntry> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn set_enabled(_entry: &StartupEntry, _enabled: bool) -> Result<(), String> {
    Err("startup entries are managed on Windows only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_flag_semantics() {
        assert!(approved_is_enabled(&[]));
        assert!(approved_is_enabled(&[2, 0, 0, 0]));
        assert!(approved_is_enabled(&[6, 0, 0, 0]));
        assert!(!approved_is_enabled(&[3, 0, 0, 0]));
        assert!(!approved_is_enabled(&[7, 0, 0, 0]));
    }

    #[test]
    fn approved_bytes_layout() {
        let on = approved_bytes(true, 123);
        assert_eq!(on, vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let off = approved_bytes(false, 0x0102030405060708);
        assert_eq!(&off[..4], &[3, 0, 0, 0]);
        assert_eq!(&off[4..], &0x0102030405060708u64.to_le_bytes());
    }

    #[test]
    fn filetime_is_after_2020() {
        // 2020-01-01 as FILETIME
        assert!(filetime_now() > 132_223_104_000_000_000);
    }
}
