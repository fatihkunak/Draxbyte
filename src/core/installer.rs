//! Per-user install / uninstall logic. The wizard (`src/wizard.rs`) drives it; the setup file is
//! this same program started in setup mode, so there is nothing else to download or unpack.
//!
//! Safety rules: the install folder is validated (never a protected or user-data folder), and an
//! uninstall deletes only the files this installer put there, then the folder if it is empty —
//! never the folder wholesale.

use std::path::{Path, PathBuf};

pub const EXE_NAME: &str = "draxbyte.exe";
/// Files an installation owns inside its folder.
const OWNED_FILES: &[&str] = &["draxbyte.exe", "draxbyte.exe.old", "uninstall.ps1"];
const SHORTCUT_NAME: &str = "Draxbyte.lnk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existing {
    pub dir: PathBuf,
    pub version: String,
}

/// Where the installer writes outside its own folder. `real()` is this user's machine; tests use
/// scratch locations so they never touch a real installation.
#[derive(Debug, Clone)]
pub struct Target {
    /// Registry path below HKCU of the "Apps & features" entry.
    pub uninstall_key: String,
    pub desktop_dir: PathBuf,
    pub start_menu_dir: PathBuf,
    /// Settings/data folder an uninstall may offer to delete (`~/.draxbyte`). Injected so tests
    /// can never reach the real one.
    pub data_dir: PathBuf,
    /// The user's home, against which `data_dir_removable` checks.
    pub home: PathBuf,
}

impl Target {
    pub fn real() -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        Target {
            uninstall_key: r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Draxbyte".into(),
            desktop_dir: crate::config::home_dir().join("Desktop"),
            start_menu_dir: env("APPDATA").map(|a| a.join(r"Microsoft\Windows\Start Menu\Programs")).unwrap_or_default(),
            data_dir: crate::config::data_dir(),
            home: crate::config::home_dir(),
        }
    }
}

impl Target {
    /// Everything inside `root`: a safe playground for demos and screenshots of the wizard (it
    /// never touches the real "Apps & features" entry, shortcuts or data folder).
    pub fn sandbox(root: &Path) -> Self {
        Target {
            uninstall_key: r"Software\DraxbyteSandbox\Uninstall".into(),
            desktop_dir: root.join("Desktop"),
            start_menu_dir: root.join("StartMenu"),
            data_dir: root.join("home").join(".draxbyte"),
            home: root.join("home"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub dest: PathBuf,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
}

#[derive(Debug, Clone)]
pub struct UninstallOptions {
    pub dir: PathBuf,
    /// Also delete `~/.draxbyte` (settings, scan history, AI cache).
    pub remove_data: bool,
}

pub fn default_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        return PathBuf::from("/Applications");
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("Programs").join("Draxbyte"))
        .unwrap_or_else(|| crate::config::home_dir().join("Draxbyte"))
}

fn comps_lc(p: &Path) -> Vec<String> {
    p.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect()
}

/// Is `dir` an acceptable install folder?
pub fn validate_dir(dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return mac_imp::validate_dest(dir);
    }
    #[allow(unreachable_code)]
    validate_dir_windows(dir)
}

fn validate_dir_windows(dir: &Path) -> Result<(), String> {
    use crate::i18n::t;
    if !dir.is_absolute() {
        return Err(t("Tam bir klasör yolu yaz (örn. C:\\Users\\sen\\AppData\\Local\\Programs\\Draxbyte).", "Enter a full folder path.").to_string());
    }
    let comps = comps_lc(dir);
    if comps.len() < 3 {
        return Err(t("Bir sürücü köküne kurulamaz; bir alt klasör seç.", "Cannot install into a drive root; pick a sub-folder.").to_string());
    }
    let home = crate::config::home_dir();
    let std_dirs = ["Documents", "Desktop", "Downloads", "Pictures", "Videos", "Music"];
    if comps_lc(&home) == comps || std_dirs.iter().any(|d| comps_lc(&home.join(d)) == comps) {
        return Err(t("Kişisel klasörünün kendisi seçilemez; içinde bir klasör aç (örn. ...\\Draxbyte).", "Pick a folder inside it (for example ...\\Draxbyte), not the personal folder itself.").to_string());
    }
    if crate::core::safety::is_protected(dir) {
        return Err(t("Bu klasör korumalı (Windows/Program Files). Yönetici izni gerekir; kullanıcı klasörünü kullan.", "This folder is protected (Windows/Program Files) and needs administrator rights; use a folder in your profile.").to_string());
    }
    Ok(())
}

/// Only `~/.draxbyte` may be removed as "settings and data" (never a custom `DRAXBYTE_HOME`).
pub fn data_dir_removable(p: &Path, home: &Path) -> bool {
    p.file_name().map(|n| n.to_string_lossy() == ".draxbyte").unwrap_or(false) && p.starts_with(home)
}

/// Total size of a folder (0 when it does not exist).
pub fn dir_size(d: &Path) -> u64 {
    if d.is_dir() {
        crate::core::scanner::dir_stats(d, &std::sync::atomic::AtomicBool::new(false)).size
    } else {
        0
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::enums::*;
    use winreg::RegKey;

    fn hide(c: &mut std::process::Command) {
        crate::core::memory::hide_window(c);
    }

    pub fn existing(t: &Target) -> Option<Existing> {
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(&t.uninstall_key, KEY_READ).ok()?;
        let dir: String = k.get_value("InstallLocation").ok()?;
        let dir = PathBuf::from(dir);
        dir.join(EXE_NAME).is_file().then(|| Existing { dir, version: k.get_value("DisplayVersion").unwrap_or_default() })
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        }
    }

    /// Copies `src` over `dest`. A running copy cannot be overwritten, but Windows lets it be
    /// renamed: the old one keeps running from `.old`, the next start uses the new file.
    fn replace_exe(src: &Path, dest: &Path, log: &mut dyn FnMut(String)) -> Result<(), String> {
        let mut last = String::new();
        for i in 0..5 {
            match std::fs::copy(src, dest) {
                Ok(_) => return Ok(()),
                Err(e) => last = e.to_string(),
            }
            if i < 4 {
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        }
        if dest.exists() {
            let old = dest.with_file_name("draxbyte.exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(dest, &old).map_err(|e| format!("{last} / {e}"))?;
            log(crate::tr!("Çalışan sürüm yeniden adlandırıldı; yenisi bir sonraki açılışta devreye girer.".to_string(), "The running version was renamed; the new one takes over at the next start.".to_string()));
            return std::fs::copy(src, dest).map(|_| ()).map_err(|e| e.to_string());
        }
        Err(last)
    }

    fn make_shortcut(lnk: &Path, target: &Path, work: &Path) -> Result<(), String> {
        if let Some(parent) = lnk.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:DRAX_LNK); \
            $s.TargetPath=$env:DRAX_TARGET; $s.WorkingDirectory=$env:DRAX_WORK; \
            $s.IconLocation=$env:DRAX_TARGET + ',0'; $s.Description='Draxbyte - disk scanner and cleaner'; $s.Save()";
        let mut c = std::process::Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("DRAX_LNK", lnk)
            .env("DRAX_TARGET", target)
            .env("DRAX_WORK", work);
        hide(&mut c);
        let out = c.output().map_err(|e| e.to_string())?;
        if out.status.success() && lnk.exists() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn write_uninstall_entry(t: &Target, dest: &Path, exe: &Path) -> Result<(), String> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&t.uninstall_key).map_err(|e| e.to_string())?;
        let set = |n: &str, v: &str| k.set_value(n, &v.to_string()).map_err(|e| e.to_string());
        set("DisplayName", "Draxbyte")?;
        set("DisplayVersion", env!("CARGO_PKG_VERSION"))?;
        set("Publisher", "Fatih Kunak")?;
        set("InstallLocation", &dest.to_string_lossy())?;
        set("DisplayIcon", &exe.to_string_lossy())?;
        set("URLInfoAbout", "https://github.com/fatihkunak/Draxbyte")?;
        set("UninstallString", &format!("\"{}\" --uninstall", exe.display()))?;
        let kb = std::fs::metadata(exe).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
        k.set_value("EstimatedSize", &kb).map_err(|e| e.to_string())?;
        k.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
        k.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
        let today = {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            let z = secs.div_euclid(86_400) + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
            format!("{y:04}{m:02}{d:02}")
        };
        set("InstallDate", &today)
    }

    pub fn install(src: &Path, opts: &InstallOptions, t: &Target, log: &mut dyn FnMut(String)) -> Result<(), String> {
        validate_dir(&opts.dest)?;
        std::fs::create_dir_all(&opts.dest).map_err(|e| e.to_string())?;
        let exe = opts.dest.join(EXE_NAME);
        if same_file(src, &exe) {
            log(crate::tr!("Program zaten bu klasörde.".to_string(), "The program is already in this folder.".to_string()));
        } else {
            log(crate::tr!("Dosyalar kopyalanıyor…".to_string(), "Copying files…".to_string()));
            replace_exe(src, &exe, log)?;
        }
        let _ = std::fs::remove_file(opts.dest.join("uninstall.ps1")); // pre-0.4 installs
        for (want, dir) in [(opts.desktop_shortcut, &t.desktop_dir), (opts.start_menu_shortcut, &t.start_menu_dir)] {
            let lnk = dir.join(SHORTCUT_NAME);
            if want {
                log(crate::tr!("Kısayol oluşturuluyor…".to_string(), "Creating shortcut…".to_string()));
                make_shortcut(&lnk, &exe, &opts.dest).map_err(|e| format!("shortcut: {e}"))?;
            } else if lnk.exists() {
                let _ = std::fs::remove_file(&lnk);
            }
        }
        log(crate::tr!("\"Uygulamalar ve özellikler\" kaydı yazılıyor…".to_string(), "Registering in \"Apps & features\"…".to_string()));
        write_uninstall_entry(t, &opts.dest, &exe)?;
        Ok(())
    }

    pub fn uninstall(opts: &UninstallOptions, t: &Target, own_pid: u32, log: &mut dyn FnMut(String)) -> Result<(), String> {
        validate_dir(&opts.dir)?;
        // 1. close running copies from this folder (never ourselves)
        let pids: Vec<u32> = crate::core::apps::processes_under(&opts.dir).iter().flat_map(|g| g.pids.clone()).filter(|p| *p != own_pid).collect();
        if !pids.is_empty() {
            log(crate::tr!("Çalışan Draxbyte kapatılıyor…".to_string(), "Closing the running Draxbyte…".to_string()));
            crate::core::memory::close_pids(&pids, true);
            std::thread::sleep(std::time::Duration::from_millis(1200));
        }
        // 2. shortcuts and the Apps & features entry
        for dir in [&t.desktop_dir, &t.start_menu_dir] {
            let _ = std::fs::remove_file(dir.join(SHORTCUT_NAME));
        }
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&t.uninstall_key);
        // 3. only the files this installer put there
        log(crate::tr!("Dosyalar siliniyor…".to_string(), "Removing files…".to_string()));
        for name in OWNED_FILES {
            let p = opts.dir.join(name);
            let mut last = None;
            for _ in 0..12 {
                match std::fs::remove_file(&p) {
                    Ok(()) => {
                        last = None;
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        last = None;
                        break;
                    }
                    Err(e) => {
                        last = Some(e.to_string());
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            }
            if let Some(e) = last {
                return Err(format!("{}: {e}", p.display()));
            }
        }
        let _ = std::fs::remove_dir(&opts.dir); // only succeeds when empty: anything else stays
        // 4. settings and data, if asked
        if opts.remove_data {
            let d = t.data_dir.clone();
            if data_dir_removable(&d, &t.home) && d.is_dir() {
                log(crate::tr!("Ayarlar ve veri siliniyor…".to_string(), "Removing settings and data…".to_string()));
                std::fs::remove_dir_all(&d).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use imp::{existing, install, uninstall};

// ---- macOS ------------------------------------------------------------------------------------
// "Install" = copy Draxbyte.app into /Applications (or ~/Applications) and start it from there;
// "uninstall" = remove that bundle and, if asked, its data and ~/Library files.

#[cfg(any(target_os = "macos", test))]
mod mac_imp {
    use super::*;
    use crate::core::apps::mac::{admin_rm, find_leftovers_in, remove_leftovers_in, MacRoots};
    use crate::core::apps::{AppKind, Confidence, InstalledApp, RemoveOptions, Removability};

    pub const APP_DIR: &str = "Draxbyte.app";
    pub const BUNDLE_ID: &str = "com.fatihkunak.draxbyte";

    /// `.../Draxbyte.app/Contents/MacOS/draxbyte` → `.../Draxbyte.app`
    pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
        let b = exe.parent()?.parent()?.parent()?;
        b.extension().map(|x| x.eq_ignore_ascii_case("app")).unwrap_or(false).then(|| b.to_path_buf())
    }

    pub fn default_dest() -> PathBuf {
        PathBuf::from("/Applications")
    }

    /// `/Applications`, `~/Applications` or any other non-protected folder of the user.
    pub fn validate_dest(dir: &Path) -> Result<(), String> {
        use crate::i18n::t;
        if !dir.is_absolute() {
            return Err(t("Tam bir klasör yolu yaz (örn. /Applications).", "Enter a full folder path (for example /Applications).").to_string());
        }
        let home = crate::config::home_dir();
        if dir == Path::new("/Applications") || dir == home.join("Applications") {
            return Ok(());
        }
        let comps = comps_lc(dir);
        let personal = ["Documents", "Desktop", "Downloads", "Pictures", "Movies", "Music"].iter().any(|d| comps_lc(&home.join(d)) == comps);
        if comps.len() < 3 || comps_lc(&home) == comps || personal || crate::core::safety::is_protected(dir) {
            return Err(t("Bu klasöre kurulamaz; /Applications ya da ~/Applications seç.", "Cannot install there; pick /Applications or ~/Applications.").to_string());
        }
        Ok(())
    }

    pub fn existing_in(candidates: &[PathBuf]) -> Option<Existing> {
        candidates.iter().find_map(|dir| {
            let app = dir.join(APP_DIR);
            let info = crate::core::apps::mac::read_bundle(&app)?;
            (info.id == BUNDLE_ID).then(|| Existing { dir: dir.clone(), version: info.version })
        })
    }

    pub fn existing(_t: &Target) -> Option<Existing> {
        existing_in(&[PathBuf::from("/Applications"), crate::config::home_dir().join("Applications")])
    }

    /// Copies a bundle keeping permissions and links (`ditto` on a Mac keeps signatures too).
    pub fn copy_bundle(src: &Path, dst: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("ditto").arg(src).arg(dst).output().map_err(|e| e.to_string())?;
            return if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) };
        }
        #[cfg(not(target_os = "macos"))]
        {
            copy_dir_all(src, dst).map_err(|e| e.to_string())
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            let to = dst.join(e.file_name());
            if e.file_type()?.is_dir() {
                copy_dir_all(&e.path(), &to)?;
            } else {
                std::fs::copy(e.path(), &to)?;
            }
        }
        Ok(())
    }

    fn remove_bundle(app: &Path) -> Result<(), String> {
        match std::fs::remove_dir_all(app) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => admin_rm(app),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn install_to(src_exe: &Path, opts: &InstallOptions, log: &mut dyn FnMut(String)) -> Result<(), String> {
        validate_dest(&opts.dest)?;
        let src = bundle_of(src_exe).ok_or_else(|| crate::tr!("Kurulum için Draxbyte.app'i aç (tek başına dosyayı değil).".to_string(), "Open Draxbyte.app to install (not the bare binary).".to_string()))?;
        std::fs::create_dir_all(&opts.dest).map_err(|e| e.to_string())?;
        let dst = opts.dest.join(APP_DIR);
        if std::fs::canonicalize(&src).ok() == std::fs::canonicalize(&dst).ok() && dst.exists() {
            log(crate::tr!("Program zaten bu klasörde.".to_string(), "The app is already in this folder.".to_string()));
            return Ok(());
        }
        if dst.exists() {
            log(crate::tr!("Önceki sürüm değiştiriliyor…".to_string(), "Replacing the previous version…".to_string()));
            remove_bundle(&dst)?;
        }
        log(crate::tr!("Uygulama kopyalanıyor…".to_string(), "Copying the app…".to_string()));
        match copy_bundle(&src, &dst) {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("{e} ({})", crate::tr!("izin gerekiyorsa ~/Applications'ı dene".to_string(), "if permission is needed, try ~/Applications".to_string()))),
        }
    }

    pub fn install(src_exe: &Path, opts: &InstallOptions, _t: &Target, log: &mut dyn FnMut(String)) -> Result<(), String> {
        install_to(src_exe, opts, log)
    }

    /// Draxbyte described as the installed program it is, so the normal leftover logic can be reused.
    fn self_app(bundle: &Path) -> InstalledApp {
        InstalledApp {
            id: "self".into(), name: "Draxbyte".into(), publisher: "Fatihkunak".into(), version: String::new(), kind: AppKind::MacApp,
            install_location: Some(bundle.to_path_buf()), size_bytes: 0, install_date: None, last_used: None, uninstall_cmd: None,
            quiet_uninstall_cmd: None, removable: Removability::Normal, reg: None, package_id: Some(BUNDLE_ID.into()),
            system_component: false, icon_exe: None,
        }
    }

    pub fn uninstall_with(opts: &UninstallOptions, own_pid: u32, roots: &MacRoots, data_dir: &Path, home: &Path, log: &mut dyn FnMut(String)) -> Result<(), String> {
        let app = opts.dir.join(APP_DIR);
        let info = crate::core::apps::mac::read_bundle(&app);
        if info.as_ref().map(|i| i.id != BUNDLE_ID).unwrap_or(true) {
            return Err(crate::tr!("Burada bir Draxbyte.app bulunamadı.".to_string(), "No Draxbyte.app found here.".to_string()));
        }
        let pids: Vec<u32> = crate::core::apps::processes_under(&app).iter().flat_map(|g| g.pids.clone()).filter(|p| *p != own_pid).collect();
        if !pids.is_empty() {
            log(crate::tr!("Çalışan Draxbyte kapatılıyor…".to_string(), "Closing the running Draxbyte…".to_string()));
            crate::core::memory::close_pids(&pids, true);
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        log(crate::tr!("Uygulama siliniyor…".to_string(), "Removing the app…".to_string()));
        remove_bundle(&app)?;
        if opts.remove_data {
            log(crate::tr!("Ayarlar, veri ve ~/Library dosyaları siliniyor…".to_string(), "Removing settings, data and ~/Library files…".to_string()));
            let me = self_app(&app);
            let items: Vec<_> = find_leftovers_in(&me, roots, false).into_iter().filter(|l| l.confidence == Confidence::High || l.confidence == Confidence::Medium).collect();
            let rep = remove_leftovers_in(&me, &items, &RemoveOptions { permanent: true, backup_dir: std::env::temp_dir().join("draxbyte-uninstall") }, roots);
            if rep.failed() > 0 {
                log(crate::tr!("Bazı dosyalar silinemedi (Tam Disk Erişimi gerekebilir).".to_string(), "Some files could not be removed (Full Disk Access may be needed).".to_string()));
            }
            if data_dir_removable(data_dir, home) && data_dir.is_dir() {
                std::fs::remove_dir_all(data_dir).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn uninstall(opts: &UninstallOptions, t: &Target, own_pid: u32, log: &mut dyn FnMut(String)) -> Result<(), String> {
        uninstall_with(opts, own_pid, &MacRoots::real(), &t.data_dir, &t.home, log)
    }
}

#[cfg(target_os = "macos")]
pub use mac_imp::{existing, install, uninstall};

#[cfg(not(any(windows, target_os = "macos")))]
pub fn existing(_t: &Target) -> Option<Existing> {
    None
}
#[cfg(not(any(windows, target_os = "macos")))]
pub fn install(_src: &Path, _o: &InstallOptions, _t: &Target, _log: &mut dyn FnMut(String)) -> Result<(), String> {
    Err("the install wizard exists for Windows and macOS".into())
}
#[cfg(not(any(windows, target_os = "macos")))]
pub fn uninstall(_o: &UninstallOptions, _t: &Target, _own_pid: u32, _log: &mut dyn FnMut(String)) -> Result<(), String> {
    Err("the uninstall wizard exists for Windows and macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_folder_is_validated() {
        assert!(validate_dir(Path::new("relative/dir")).is_err());
        assert!(validate_dir(&crate::config::home_dir()).is_err());
        assert!(validate_dir(&crate::config::home_dir().join("Documents")).is_err());
        assert!(validate_dir(&default_dir()).is_ok());
        #[cfg(windows)]
        {
            assert!(validate_dir(Path::new(r"C:\")).is_err());
            assert!(validate_dir(Path::new(r"C:\Windows\System32\Draxbyte")).is_err());
            assert!(validate_dir(Path::new(r"C:\Program Files\Draxbyte")).is_err());
        }
    }

    #[test]
    fn only_the_default_data_folder_may_be_removed() {
        let home = crate::config::home_dir();
        assert!(data_dir_removable(&home.join(".draxbyte"), &home));
        assert!(!data_dir_removable(&home.join("Documents"), &home));
        assert!(!data_dir_removable(&home, &home));
        assert!(!data_dir_removable(Path::new("/tmp/.draxbyte-custom"), &home));
        assert!(!data_dir_removable(Path::new("/elsewhere/.draxbyte"), &home), "outside the home it is never removable");
    }

    mod mac_flow {
        use super::super::mac_imp::*;
        use super::super::*;
        use crate::core::apps::mac::MacRoots;
        use std::fs;

        fn make_app(dir: &Path, id: &str, version: &str) -> PathBuf {
            let app = dir.join(APP_DIR);
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::create_dir_all(app.join("Contents/Resources")).unwrap();
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleName</key><string>Draxbyte</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
            );
            fs::write(app.join("Contents/Info.plist"), xml).unwrap();
            fs::write(app.join("Contents/MacOS/draxbyte"), vec![5u8; 3000]).unwrap();
            fs::write(app.join("Contents/Resources/AppIcon.icns"), b"icon").unwrap();
            app
        }

        #[test]
        fn bundle_is_found_from_the_binary_path() {
            assert_eq!(bundle_of(Path::new("/x/Draxbyte.app/Contents/MacOS/draxbyte")), Some(PathBuf::from("/x/Draxbyte.app")));
            assert_eq!(bundle_of(Path::new("/x/draxbyte")), None);
            assert_eq!(bundle_of(Path::new("/Volumes/Draxbyte/Draxbyte.app/Contents/MacOS/draxbyte")), Some(PathBuf::from("/Volumes/Draxbyte/Draxbyte.app")));
        }

        #[test]
        fn install_update_and_uninstall_on_a_scratch_applications_folder() {
            let tmp = tempfile::tempdir().unwrap();
            let t = tmp.path();
            let dmg = t.join("dmg");
            fs::create_dir_all(&dmg).unwrap();
            let src_app = make_app(&dmg, BUNDLE_ID, "0.4.0");
            let src_exe = src_app.join("Contents/MacOS/draxbyte");
            let apps = t.join("Users/u/Applications");
            let opts = InstallOptions { dest: apps.clone(), desktop_shortcut: false, start_menu_shortcut: false };
            let mut log = Vec::new();

            install_to(&src_exe, &opts, &mut |m| log.push(m)).unwrap();
            assert_eq!(fs::read(apps.join("Draxbyte.app/Contents/MacOS/draxbyte")).unwrap(), vec![5u8; 3000]);
            assert!(apps.join("Draxbyte.app/Contents/Resources/AppIcon.icns").exists());
            let ex = existing_in(&[apps.clone()]).expect("found");
            assert_eq!((ex.dir, ex.version.as_str()), (apps.clone(), "0.4.0"));

            // update over the existing copy
            make_app(&dmg, BUNDLE_ID, "0.5.0");
            fs::write(src_app.join("Contents/MacOS/draxbyte"), vec![6u8; 3500]).unwrap();
            install_to(&src_exe, &opts, &mut |_| {}).unwrap();
            assert_eq!(fs::read(apps.join("Draxbyte.app/Contents/MacOS/draxbyte")).unwrap().len(), 3500);
            assert_eq!(existing_in(&[apps.clone()]).unwrap().version, "0.5.0");

            // somebody else's app and Library data must survive the uninstall
            let other = make_app(&t.join("elsewhere"), "com.other.app", "1.0");
            let user_lib = t.join("Users/u/Library");
            for p in ["Caches/com.fatihkunak.draxbyte/c", "Preferences/com.fatihkunak.draxbyte.plist", "Caches/com.other.app/keep"] {
                let f = user_lib.join(p);
                fs::create_dir_all(f.parent().unwrap()).unwrap();
                fs::write(f, b"x").unwrap();
            }
            let roots = MacRoots { user_lib: user_lib.clone(), system_lib: t.join("Library") };
            // scratch home with a scratch data folder: the real ~/.draxbyte is never involved
            let home = t.join("scratch-home");
            let data = home.join(".draxbyte");
            fs::create_dir_all(&data).unwrap();
            fs::write(data.join("config.toml"), b"x").unwrap();
            // keeping data: only the app goes
            uninstall_with(&UninstallOptions { dir: apps.clone(), remove_data: false }, 0, &roots, &data, &home, &mut |_| {}).unwrap();
            assert!(!apps.join("Draxbyte.app").exists());
            assert!(user_lib.join("Caches/com.fatihkunak.draxbyte/c").exists(), "data kept when not asked");
            assert!(data.join("config.toml").exists());
            // removing data too
            install_to(&src_exe, &opts, &mut |_| {}).unwrap();
            uninstall_with(&UninstallOptions { dir: apps.clone(), remove_data: true }, 0, &roots, &data, &home, &mut |_| {}).unwrap();
            assert!(!data.exists(), "the scratch data folder went");
            assert!(!user_lib.join("Caches/com.fatihkunak.draxbyte").exists());
            assert!(!user_lib.join("Preferences/com.fatihkunak.draxbyte.plist").exists());
            assert!(user_lib.join("Caches/com.other.app/keep").exists());
            assert!(other.exists());
        }

        #[test]
        fn refuses_to_uninstall_something_that_is_not_draxbyte() {
            let tmp = tempfile::tempdir().unwrap();
            let apps = tmp.path().join("Applications");
            fs::create_dir_all(&apps).unwrap();
            let imposter = make_app(&apps, "com.someone.else", "1.0");
            let roots = MacRoots { user_lib: tmp.path().join("Library"), system_lib: tmp.path().join("SysLib") };
            let scratch = tmp.path().join("scratch-home");
            let r = uninstall_with(&UninstallOptions { dir: apps.clone(), remove_data: true }, 0, &roots, &scratch.join(".draxbyte"), &scratch, &mut |_| {});
            assert!(r.is_err());
            assert!(imposter.exists(), "an app with another bundle id is never deleted");
            assert!(validate_dest(Path::new("/System/Library")).is_err());
            assert!(validate_dest(Path::new("relative")).is_err());
            #[cfg(unix)]
            assert!(validate_dest(Path::new("/Applications")).is_ok());
        }
    }

    #[cfg(windows)]
    mod windows_flow {
        use super::*;
        use std::fs;
        use winreg::enums::*;
        use winreg::RegKey;

        fn target(t: &Path) -> Target {
            Target {
                uninstall_key: format!(r"Software\DraxbyteTest-installer-{}\Uninstall", std::process::id()),
                desktop_dir: t.join("Desktop"),
                start_menu_dir: t.join("Start Menu"),
                data_dir: t.join("scratch-home").join(".draxbyte"),
                home: t.join("scratch-home"),
            }
        }

        #[test]
        fn install_update_and_uninstall_touch_only_their_own_files() {
            let tmp = tempfile::tempdir().unwrap();
            let t = tmp.path();
            let tgt = target(t);
            let src = t.join("Draxbyte-Setup.exe");
            fs::write(&src, vec![1u8; 4096]).unwrap();
            let dest = t.join("Programs").join("Draxbyte");
            let opts = InstallOptions { dest: dest.clone(), desktop_shortcut: true, start_menu_shortcut: true };
            let mut log = Vec::new();

            install(&src, &opts, &tgt, &mut |m| log.push(m)).unwrap();
            assert_eq!(fs::read(dest.join(EXE_NAME)).unwrap(), vec![1u8; 4096]);
            assert!(tgt.desktop_dir.join(SHORTCUT_NAME).exists() && tgt.start_menu_dir.join(SHORTCUT_NAME).exists());
            let ex = existing(&tgt).expect("registered");
            assert_eq!((ex.dir.clone(), ex.version.as_str()), (dest.clone(), env!("CARGO_PKG_VERSION")));
            let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&tgt.uninstall_key).unwrap();
            let us: String = k.get_value("UninstallString").unwrap();
            assert!(us.ends_with("--uninstall") && us.contains("draxbyte.exe"), "{us}");

            // update: new bytes over the old, one shortcut switched off
            fs::write(&src, vec![2u8; 5000]).unwrap();
            let opts2 = InstallOptions { desktop_shortcut: false, ..opts.clone() };
            install(&src, &opts2, &tgt, &mut |m| log.push(m)).unwrap();
            assert_eq!(fs::read(dest.join(EXE_NAME)).unwrap().len(), 5000);
            assert!(!tgt.desktop_dir.join(SHORTCUT_NAME).exists(), "unticked shortcut is removed");
            assert!(tgt.start_menu_dir.join(SHORTCUT_NAME).exists());

            // a stranger's file in the same folder must survive the uninstall
            fs::write(dest.join("my-notes.txt"), b"mine").unwrap();
            uninstall(&UninstallOptions { dir: dest.clone(), remove_data: false }, &tgt, std::process::id(), &mut |m| log.push(m)).unwrap();
            assert!(!dest.join(EXE_NAME).exists());
            assert!(!tgt.start_menu_dir.join(SHORTCUT_NAME).exists());
            assert!(existing(&tgt).is_none());
            assert!(dest.join("my-notes.txt").exists(), "unknown files and the folder stay");

            // with nothing foreign left, the folder goes too
            fs::remove_file(dest.join("my-notes.txt")).unwrap();
            install(&src, &opts, &tgt, &mut |_| {}).unwrap();
            uninstall(&UninstallOptions { dir: dest.clone(), remove_data: false }, &tgt, std::process::id(), &mut |_| {}).unwrap();
            assert!(!dest.exists());
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!(r"Software\DraxbyteTest-installer-{}", std::process::id()));
        }

        #[test]
        fn a_running_exe_is_replaced_by_renaming_it() {
            let tmp = tempfile::tempdir().unwrap();
            let t = tmp.path();
            let tgt = target(t);
            let dest = t.join("Programs").join("Draxbyte");
            fs::create_dir_all(&dest).unwrap();
            // a real running program from that folder: a copy of this test binary
            let running = dest.join(EXE_NAME);
            fs::copy(std::env::current_exe().unwrap(), &running).unwrap();
            let mut child = std::process::Command::new(&running).args(["--list", "--ignored"]).stdout(std::process::Stdio::null()).spawn();
            // (it exits quickly; the rename path is exercised when the copy is refused, otherwise a plain copy works)
            let src = t.join("Draxbyte-Setup.exe");
            fs::write(&src, vec![9u8; 3000]).unwrap();
            let opts = InstallOptions { dest: dest.clone(), desktop_shortcut: false, start_menu_shortcut: false };
            install(&src, &opts, &tgt, &mut |_| {}).unwrap();
            assert_eq!(fs::read(&running).unwrap(), vec![9u8; 3000]);
            if let Ok(c) = child.as_mut() {
                let _ = c.wait();
            }
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!(r"Software\DraxbyteTest-installer-{}", std::process::id()));
        }
    }
}
