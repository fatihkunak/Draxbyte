//! macOS: apps are `.app` bundles. Lists them (with Spotlight's last-used date), removes a bundle
//! and the files that belong to its bundle identifier under `~/Library` and `/Library`.
//!
//! Apps that macOS itself ships on the sealed, read-only system volume (Safari, Mail, Music…)
//! cannot be deleted by any program: they are listed as `Sealed`, and only their caches and
//! temporary data are offered for cleaning.
//!
//! The logic is written against plain paths so it is unit-tested on every platform; only the few
//! calls to `mdls` / `osascript` / `tmutil` need a Mac.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::*;
use crate::core::scanner::dir_stats;
use std::sync::atomic::AtomicBool;

/// Where leftovers live. `real()` is this Mac; tests point it at scratch folders.
#[derive(Debug, Clone)]
pub struct MacRoots {
    pub user_lib: PathBuf,
    pub system_lib: PathBuf,
}

impl MacRoots {
    pub fn real() -> Self {
        MacRoots { user_lib: crate::config::home_dir().join("Library"), system_lib: PathBuf::from("/Library") }
    }
}

/// Library sub-folders a program may keep files in, and whether they hold only caches.
const USER_DIRS: &[(&str, bool)] = &[
    ("Application Support", false),
    ("Caches", true),
    ("Logs", true),
    ("Preferences", false),
    ("Containers", false),
    ("Group Containers", false),
    ("Saved Application State", true),
    ("HTTPStorages", true),
    ("WebKit", true),
    ("Cookies", false),
    ("Application Scripts", false),
    ("LaunchAgents", false),
];
const SYSTEM_DIRS: &[(&str, bool)] = &[
    ("Application Support", false),
    ("Caches", true),
    ("Logs", true),
    ("Preferences", false),
    ("LaunchAgents", false),
    ("LaunchDaemons", false),
    ("PrivilegedHelperTools", false),
];

pub struct BundleInfo {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Reads `Contents/Info.plist` (XML or binary).
pub fn read_bundle(bundle: &Path) -> Option<BundleInfo> {
    let v = plist::Value::from_file(bundle.join("Contents").join("Info.plist")).ok()?;
    let d = v.as_dictionary()?;
    let s = |k: &str| d.get(k).and_then(|x| x.as_string()).unwrap_or_default().to_string();
    let id = s("CFBundleIdentifier");
    if id.is_empty() {
        return None;
    }
    let folder = bundle.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = [s("CFBundleDisplayName"), s("CFBundleName"), folder].into_iter().find(|n| !n.is_empty()).unwrap_or_default();
    let version = [s("CFBundleShortVersionString"), s("CFBundleVersion")].into_iter().find(|v| !v.is_empty()).unwrap_or_default();
    Some(BundleInfo { id, name, version })
}

/// `com.vendor.app` → `Vendor` (best available guess of the publisher).
pub fn publisher_from_id(id: &str) -> String {
    let mut parts = id.split('.');
    parts.next();
    let vendor = parts.next().unwrap_or_default();
    let mut c = vendor.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// `2026-10-01 12:34:56 +0000` (what `mdls` prints) → unix seconds.
pub fn parse_mdls_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s.starts_with('(') {
        return None;
    }
    let (date, rest) = s.split_once(' ')?;
    let (time, tz) = rest.split_once(' ').unwrap_or((rest, "+0000"));
    let day = parse_install_date(&date.replace('-', ""))?;
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (h, m, sec) = (t.next()??, t.next()??, t.next()??);
    let sign = if tz.starts_with('-') { -1 } else { 1 };
    let tz = tz.trim_start_matches(['+', '-']);
    let off = if tz.len() == 4 { tz[..2].parse::<i64>().ok()? * 3600 + tz[2..].parse::<i64>().ok()? * 60 } else { 0 };
    Some(day + h * 3600 + m * 60 + sec - sign * off)
}

fn last_used(bundle: &Path) -> Option<i64> {
    let out = std::process::Command::new("mdls")
        .args(["-raw", "-name", "kMDItemLastUsedDate"])
        .arg(bundle)
        .output()
        .ok()?;
    out.status.success().then(|| parse_mdls_date(&String::from_utf8_lossy(&out.stdout))).flatten()
}

fn dirs_to_scan() -> Vec<PathBuf> {
    let home = crate::config::home_dir();
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        home.join("Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ]
}

/// All `.app` bundles in the given folders (one level of sub-folders is searched too).
pub fn find_bundles(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let is_app = p.extension().map(|x| x.eq_ignore_ascii_case("app")).unwrap_or(false);
            if is_app {
                if seen.insert(std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone())) {
                    out.push(p);
                }
            } else if e.file_type().map(|t| t.is_dir()).unwrap_or(false) && dir.ends_with("Applications") {
                // e.g. /Applications/Adobe Photoshop 2025/Adobe Photoshop 2025.app
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for ie in inner.flatten() {
                        let ip = ie.path();
                        if ip.extension().map(|x| x.eq_ignore_ascii_case("app")).unwrap_or(false)
                            && seen.insert(std::fs::canonicalize(&ip).unwrap_or_else(|_| ip.clone()))
                        {
                            out.push(ip);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Is the bundle on the sealed system volume? (`/Applications/Safari.app` is a link into it.)
pub fn is_sealed(bundle: &Path) -> bool {
    let real = std::fs::canonicalize(bundle).unwrap_or_else(|_| bundle.to_path_buf());
    let s = real.to_string_lossy();
    s.starts_with("/System/")
}

fn to_app(bundle: &Path, with_usage: bool) -> Option<InstalledApp> {
    let info = read_bundle(bundle)?;
    let sealed = is_sealed(bundle);
    Some(InstalledApp {
        id: format!("mac:{}", bundle.display()),
        name: info.name,
        publisher: publisher_from_id(&info.id),
        version: info.version,
        kind: AppKind::MacApp,
        install_location: Some(bundle.to_path_buf()),
        size_bytes: 0,
        install_date: std::fs::metadata(bundle).ok().and_then(|m| m.created().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64),
        last_used: if with_usage { last_used(bundle) } else { None },
        uninstall_cmd: None,
        quiet_uninstall_cmd: None,
        removable: if sealed {
            Removability::Sealed(crate::tr!(
                "macOS'un mühürlü, salt okunur sistem biriminde; hiçbir uygulama silemez (yalnızca önbelleği temizlenir)".to_string(),
                "Lives on macOS's sealed, read-only system volume; no app can delete it (only its caches are cleaned)".to_string()
            ))
        } else {
            Removability::Normal
        },
        reg: None,
        package_id: Some(info.id),
        system_component: false,
        icon_exe: None,
    })
}

pub fn inventory() -> Vec<InstalledApp> {
    let mut apps: Vec<InstalledApp> = find_bundles(&dirs_to_scan()).iter().filter_map(|b| to_app(b, true)).collect();
    let cancel = AtomicBool::new(false);
    for a in apps.iter_mut() {
        if let Some(loc) = a.install_location.as_deref() {
            a.size_bytes = dir_stats(loc, &cancel).size;
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

// ---- leftovers ------------------------------------------------------------------------------

fn path_size(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => dir_stats(p, &AtomicBool::new(false)).size,
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Does a Library entry name belong to this bundle id?  `id`, `id.plist`, `id.savedState`,
/// `id.helper`… (a following dot), group containers that embed the id (`group.<id>`, `TEAM.<id>`).
fn id_matches(entry: &str, id: &str, group_container: bool) -> bool {
    let (e, i) = (entry.to_lowercase(), id.to_lowercase());
    e == i || e.starts_with(&format!("{i}.")) || (group_container && (e.ends_with(&format!(".{i}")) || e.contains(&i)))
}

/// Everything under the Library folders that belongs to `app`. `cache_only` limits it to caches,
/// logs and saved state (used for sealed system apps).
pub fn find_leftovers_in(app: &InstalledApp, roots: &MacRoots, cache_only: bool) -> Vec<Leftover> {
    let Some(id) = app.package_id.as_deref() else { return Vec::new() };
    let variants = name_variants(&app.name);
    let mut out: Vec<Leftover> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (base, table) in [(&roots.user_lib, USER_DIRS), (&roots.system_lib, SYSTEM_DIRS)] {
        for (sub, caches) in table {
            if cache_only && !caches {
                continue;
            }
            let dir = base.join(sub);
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let by_id = id_matches(&name, id, *sub == "Group Containers");
                // App Support / Caches / Logs are often named after the app instead of the id.
                let by_name = matches!(*sub, "Application Support" | "Caches" | "Logs")
                    && variants.iter().any(|v| *v == norm_key(&name));
                if !by_id && !by_name {
                    continue;
                }
                let p = e.path();
                if !mac_path_allowed(&p, app, roots) || !seen.insert(p.clone()) {
                    continue;
                }
                let holds_data = matches!(*sub, "Containers" | "Group Containers");
                out.push(Leftover {
                    kind: LeftoverKind::Path { is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false) },
                    label: p.to_string_lossy().into_owned(),
                    size: path_size(&p),
                    confidence: if holds_data { Confidence::Medium } else { Confidence::High },
                    why: if holds_data {
                        "sandbox data; may contain documents you created".into()
                    } else if by_id {
                        "named after the program's bundle identifier".into()
                    } else {
                        "named after the program".into()
                    },
                });
            }
        }
    }
    out.sort_by(|a, b| a.confidence.cmp(&b.confidence).then(b.size.cmp(&a.size)));
    out
}

/// What the app listing/removal flow shows: the bundle itself plus its Library files.
pub fn find_leftovers(app: &InstalledApp, _others: &[InstalledApp]) -> Vec<Leftover> {
    let sealed = matches!(app.removable, Removability::Sealed(_));
    let mut v = find_leftovers_in(app, &MacRoots::real(), sealed);
    if let Some(b) = app.install_location.as_deref().filter(|b| b.exists() && !sealed) {
        v.insert(
            0,
            Leftover {
                kind: LeftoverKind::Path { is_dir: true },
                label: b.to_string_lossy().into_owned(),
                size: path_size(b),
                confidence: Confidence::High,
                why: "the app itself".into(),
            },
        );
    }
    v
}

/// May this path be deleted as part of removing `app`?
pub fn mac_path_allowed(p: &Path, app: &InstalledApp, roots: &MacRoots) -> bool {
    let sealed = matches!(app.removable, Removability::Sealed(_));
    if let Some(b) = app.install_location.as_deref() {
        if comps_lc(p) == comps_lc(b) {
            return !sealed && b.extension().map(|x| x.eq_ignore_ascii_case("app")).unwrap_or(false);
        }
    }
    let mut ok_dirs: Vec<(PathBuf, bool)> = USER_DIRS.iter().map(|(d, c)| (roots.user_lib.join(d), *c)).collect();
    ok_dirs.extend(SYSTEM_DIRS.iter().map(|(d, c)| (roots.system_lib.join(d), *c)));
    for (dir, caches_only) in ok_dirs {
        let Some(depth) = depth_below(p, &dir) else { continue };
        if depth == 0 {
            return false; // the Library sub-folder itself
        }
        if sealed && !caches_only {
            return false;
        }
        // Apple's own data is only touchable for the Apple app being removed itself (GarageBand,
        // iMovie…); never for another app and never another Apple app's folder.
        let first = comps_lc(p).get(comps_lc(&dir).len()).cloned().unwrap_or_default();
        if first.starts_with("com.apple.") {
            let own = app.package_id.as_deref().map(|i| id_matches(&first, i, true)).unwrap_or(false);
            if !own {
                return false;
            }
        }
        return true;
    }
    false
}

// ---- removal ---------------------------------------------------------------------------------

/// Removes the `.app` bundle itself. A bundle the user may delete goes at once; one owned by
/// root asks for the administrator password through the system dialog.
pub fn remove_bundle(app: &InstalledApp) -> Result<i32, String> {
    let b = app.install_location.as_deref().ok_or("no bundle path")?;
    if matches!(app.removable, Removability::Sealed(_)) {
        return Err("sealed system app".into());
    }
    if !b.extension().map(|x| x.eq_ignore_ascii_case("app")).unwrap_or(false) {
        return Err("not an .app bundle".into());
    }
    match std::fs::remove_dir_all(b) {
        Ok(()) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => admin_rm(b).map(|_| 0),
        Err(e) => Err(e.to_string()),
    }
}

pub fn admin_rm(p: &Path) -> Result<(), String> {
    // The path goes through an environment variable so quotes in it cannot break the script.
    let script = "do shell script \"/bin/rm -R \" & quoted form of (system attribute \"DRAXBYTE_PATH\") with administrator privileges";
    let out = std::process::Command::new("osascript")
        .args(["-e", script])
        .env("DRAXBYTE_PATH", p)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Stops a LaunchAgent/Daemon before its plist is deleted (best effort).
fn unload_launchd(p: &Path) {
    let s = p.to_string_lossy();
    if s.contains("/LaunchAgents/") || s.contains("/LaunchDaemons/") {
        let _ = std::process::Command::new("launchctl").arg("unload").arg(p).output();
    }
}

pub fn remove_leftovers(app: &InstalledApp, items: &[Leftover], opts: &RemoveOptions) -> RemoveReport {
    remove_leftovers_in(app, items, opts, &MacRoots::real())
}

pub fn remove_leftovers_in(app: &InstalledApp, items: &[Leftover], opts: &RemoveOptions, roots: &MacRoots) -> RemoveReport {
    let mut rep = RemoveReport { backup_dir: opts.backup_dir.clone(), ..Default::default() };
    for it in items {
        let res: Result<(), String> = match &it.kind {
            LeftoverKind::Path { .. } => {
                let p = Path::new(&it.label);
                if !mac_path_allowed(p, app, roots) {
                    Err(crate::tr!("korumalı yol, atlandı".to_string(), "protected path, skipped".to_string()))
                } else if std::fs::symlink_metadata(p).is_err() {
                    Ok(())
                } else if comps_lc(p) == app.install_location.as_deref().map(comps_lc).unwrap_or_default() {
                    remove_bundle(app).map(|_| ())
                } else {
                    unload_launchd(p);
                    delete(p, opts.permanent)
                }
            }
            _ => Err("not applicable on macOS".into()),
        };
        if res.is_ok() {
            rep.freed += it.size;
        }
        rep.items.push(ItemResult { label: it.label.clone(), ok: res.is_ok(), message: res.err().unwrap_or_default() });
    }
    rep
}

fn delete(p: &Path, permanent: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    if !permanent {
        return trash::delete(p).map_err(|e| e.to_string());
    }
    let r = if meta.is_dir() && !meta.file_type().is_symlink() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
    match r {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !p.starts_with(crate::config::home_dir()) => admin_rm(p),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(crate::tr!(
            "izin yok — Sistem Ayarları → Gizlilik ve Güvenlik → Tam Disk Erişimi'ne Draxbyte'ı ekle".to_string(),
            "permission denied — add Draxbyte under System Settings → Privacy & Security → Full Disk Access".to_string()
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// A local Time Machine snapshot: a safety net before removing programs (no password needed).
pub fn snapshot() -> Result<(), String> {
    let out = std::process::Command::new("tmutil").arg("localsnapshot").output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("failed").trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_plist(app: &Path, id: &str, name: &str) {
        let c = app.join("Contents");
        fs::create_dir_all(&c).unwrap();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundleShortVersionString</key><string>3.2</string>
</dict></plist>"#
        );
        fs::write(c.join("Info.plist"), xml).unwrap();
        fs::write(c.join("exe"), vec![0u8; 2048]).unwrap();
    }

    #[test]
    fn bundle_info_and_publisher() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("Zorb Notes.app");
        write_plist(&app, "com.zorblabs.notes", "Zorb Notes");
        let i = read_bundle(&app).unwrap();
        assert_eq!((i.id.as_str(), i.name.as_str(), i.version.as_str()), ("com.zorblabs.notes", "Zorb Notes", "3.2"));
        assert_eq!(publisher_from_id("com.zorblabs.notes"), "Zorblabs");
        assert!(read_bundle(&tmp.path().join("Nope.app")).is_none());
    }

    #[test]
    fn mdls_dates_parse() {
        assert_eq!(parse_mdls_date("2025-01-01 00:00:00 +0000"), Some(1_735_689_600));
        assert_eq!(parse_mdls_date("2025-01-01 03:00:00 +0300"), Some(1_735_689_600));
        assert_eq!(parse_mdls_date("(null)"), None);
        assert_eq!(parse_mdls_date(""), None);
    }

    #[test]
    fn finds_bundles_in_folders_and_one_level_down() {
        let tmp = tempfile::tempdir().unwrap();
        let apps = tmp.path().join("Applications");
        write_plist(&apps.join("A.app"), "com.x.a", "A");
        write_plist(&apps.join("Suite 2025/B.app"), "com.x.b", "B");
        fs::create_dir_all(apps.join("NotAnApp")).unwrap();
        let found = find_bundles(&[apps.clone()]);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    fn scratch_app(bundle: &Path, id: &str, name: &str, sealed: bool) -> InstalledApp {
        InstalledApp {
            id: "t".into(), name: name.into(), publisher: String::new(), version: String::new(), kind: AppKind::MacApp,
            install_location: Some(bundle.to_path_buf()), size_bytes: 0, install_date: None, last_used: None,
            uninstall_cmd: None, quiet_uninstall_cmd: None,
            removable: if sealed { Removability::Sealed("sealed".into()) } else { Removability::Normal },
            reg: None, package_id: Some(id.into()), system_component: false, icon_exe: None,
        }
    }

    fn lib(t: &Path) -> MacRoots {
        let u = t.join("home/Library");
        let s = t.join("Library");
        for d in ["Application Support", "Caches", "Preferences", "Containers", "Group Containers", "Saved Application State", "Logs", "LaunchAgents"] {
            fs::create_dir_all(u.join(d)).unwrap();
        }
        for d in ["Application Support", "Caches", "LaunchDaemons"] {
            fs::create_dir_all(s.join(d)).unwrap();
        }
        MacRoots { user_lib: u, system_lib: s }
    }

    #[test]
    fn leftovers_are_found_by_bundle_id_and_name_and_neighbours_survive() {
        let tmp = tempfile::tempdir().unwrap();
        let r = lib(tmp.path());
        let ul = &r.user_lib;
        let app = scratch_app(&tmp.path().join("Applications/Zorb Notes.app"), "com.zorblabs.notes", "Zorb Notes", false);
        for p in [
            "Application Support/Zorb Notes/db.sqlite", "Caches/com.zorblabs.notes/x.bin", "Preferences/com.zorblabs.notes.plist",
            "Containers/com.zorblabs.notes/Data/doc.txt", "Group Containers/ABCDE12345.com.zorblabs.notes/f", "Saved Application State/com.zorblabs.notes.savedState/w",
            "LaunchAgents/com.zorblabs.notes.helper.plist", "Logs/Zorb Notes/a.log",
            // must stay
            "Application Support/Other App/keep", "Caches/com.zorblabs.notesfoo/keep", "Preferences/com.other.app.plist",
            "Application Support/com.apple.Safari/keep", "Caches/com.apple.Safari/keep",
        ] {
            let f = ul.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, b"data").unwrap();
        }
        fs::create_dir_all(r.system_lib.join("Application Support/com.zorblabs.notes")).unwrap();
        let found = find_leftovers_in(&app, &r, false);
        let labels: Vec<String> = found.iter().map(|l| l.label.replace('\\', "/")).collect();
        let has = |s: &str| labels.iter().any(|l| l.ends_with(s));
        for want in [
            "Application Support/Zorb Notes", "Caches/com.zorblabs.notes", "Preferences/com.zorblabs.notes.plist",
            "Containers/com.zorblabs.notes", "Group Containers/ABCDE12345.com.zorblabs.notes", "Saved Application State/com.zorblabs.notes.savedState",
            "LaunchAgents/com.zorblabs.notes.helper.plist", "Logs/Zorb Notes", "Library/Application Support/com.zorblabs.notes",
        ] {
            assert!(has(want), "missing {want}: {labels:?}");
        }
        for unwanted in ["Other App", "notesfoo", "com.other.app", "com.apple.Safari"] {
            assert!(!labels.iter().any(|l| l.contains(unwanted)), "must not offer {unwanted}: {labels:?}");
        }
        // sandbox containers may hold documents: probable, not pre-selected
        let cont = found.iter().find(|l| l.label.replace('\\', "/").ends_with("Containers/com.zorblabs.notes")).unwrap();
        assert_eq!(cont.confidence, Confidence::Medium);

        let rep = remove_leftovers_in(&app, &found.iter().filter(|l| l.confidence == Confidence::High).cloned().collect::<Vec<_>>(), &RemoveOptions { permanent: true, backup_dir: tmp.path().join("b") }, &r);
        assert_eq!(rep.failed(), 0, "{:?}", rep.items.iter().filter(|i| !i.ok).collect::<Vec<_>>());
        assert!(!ul.join("Caches/com.zorblabs.notes").exists());
        assert!(!ul.join("Preferences/com.zorblabs.notes.plist").exists());
        assert!(ul.join("Containers/com.zorblabs.notes/Data/doc.txt").exists(), "probable item untouched");
        assert!(ul.join("Application Support/Other App/keep").exists());
        assert!(ul.join("Caches/com.apple.Safari/keep").exists());
        assert!(ul.join("Caches/com.zorblabs.notesfoo/keep").exists());
    }

    #[test]
    fn sealed_system_app_only_offers_caches_and_never_apple_data_to_others() {
        let tmp = tempfile::tempdir().unwrap();
        let r = lib(tmp.path());
        let ul = &r.user_lib;
        for p in ["Caches/com.apple.Safari/c.bin", "Application Support/com.apple.Safari/bookmarks", "Preferences/com.apple.Safari.plist", "Containers/com.apple.Safari/d"] {
            let f = ul.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, b"x").unwrap();
        }
        let safari = scratch_app(Path::new("/System/Cryptexes/App/System/Applications/Safari.app"), "com.apple.Safari", "Safari", true);
        let found = find_leftovers_in(&safari, &r, true);
        let labels: Vec<String> = found.iter().map(|l| l.label.replace('\\', "/")).collect();
        assert!(labels.iter().any(|l| l.ends_with("Caches/com.apple.Safari")), "{labels:?}");
        assert!(!labels.iter().any(|l| l.contains("Application Support") || l.contains("Preferences") || l.contains("Containers")), "{labels:?}");
        // the sealed bundle itself can never be a deletion target
        assert!(!mac_path_allowed(Path::new("/System/Cryptexes/App/System/Applications/Safari.app"), &safari, &r));
        // a third-party app may not touch Apple's data
        let other = scratch_app(Path::new("/Applications/Other.app"), "com.zorb.other", "Other", false);
        assert!(!mac_path_allowed(&ul.join("Caches/com.apple.Safari"), &other, &r));
        // library roots themselves and arbitrary folders are refused
        assert!(!mac_path_allowed(&ul.join("Caches"), &other, &r));
        assert!(!mac_path_allowed(&ul.join("Documents/x"), &other, &r));
    }

    #[test]
    fn removable_apple_app_may_clear_its_own_data() {
        let tmp = tempfile::tempdir().unwrap();
        let r = lib(tmp.path());
        let gb = scratch_app(&tmp.path().join("Applications/GarageBand.app"), "com.apple.garageband10", "GarageBand", false);
        assert!(mac_path_allowed(&r.user_lib.join("Application Support/com.apple.garageband10"), &gb, &r));
        assert!(!mac_path_allowed(&r.user_lib.join("Application Support/com.apple.Safari"), &gb, &r));
    }
}
