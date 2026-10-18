//! Windows: program list (registry "Uninstall" keys + Store packages), a last-use guess from
//! UserAssist, launching the vendor uninstaller, finding what it leaves behind, and removing
//! that with registry backups so the registry part can be undone.

use super::*;
use crate::core::scanner::dir_stats;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";

fn predef(h: Hive) -> RegKey {
    RegKey::predef(match h {
        Hive::User => HKEY_CURRENT_USER,
        Hive::Machine => HKEY_LOCAL_MACHINE,
    })
}

fn hive_name(h: Hive) -> &'static str {
    match h {
        Hive::User => "HKCU",
        Hive::Machine => "HKLM",
    }
}

fn hide(c: &mut std::process::Command) {
    crate::core::memory::hide_window(c);
}

/// Where leftovers are looked for. `system()` is the real machine; tests point it at scratch folders.
#[derive(Debug, Clone)]
pub struct ScanEnv {
    pub roots: Roots,
    /// (hive, path) registry branches that hold `<Vendor>\<Product>` keys.
    pub reg_roots: Vec<(Hive, String)>,
    pub services: bool,
    pub tasks: bool,
}

impl ScanEnv {
    pub fn system() -> Self {
        ScanEnv {
            roots: Roots::from_env(),
            reg_roots: vec![
                (Hive::User, "Software".into()),
                (Hive::Machine, "SOFTWARE".into()),
                (Hive::Machine, r"SOFTWARE\WOW6432Node".into()),
            ],
            services: true,
            tasks: true,
        }
    }
}

// ---- inventory ------------------------------------------------------------------------------

fn s(k: &RegKey, name: &str) -> String {
    k.get_value::<String, _>(name).unwrap_or_default().trim().to_string()
}
fn d(k: &RegKey, name: &str) -> Option<u32> {
    k.get_value::<u32, _>(name).ok()
}

fn unquote(p: &str) -> String {
    p.trim().trim_matches('"').trim().to_string()
}

/// `C:\x\app.exe,0` → `C:\x\app.exe`
fn icon_path(icon: &str) -> Option<PathBuf> {
    let p = unquote(icon.rsplit_once(',').map(|(a, b)| if b.trim().parse::<i32>().is_ok() { a } else { icon }).unwrap_or(icon));
    (!p.is_empty()).then(|| PathBuf::from(p))
}

fn read_entry(hive: Hive, base: &str, sub: &str, k: &RegKey) -> Option<InstalledApp> {
    let name = s(k, "DisplayName");
    if name.is_empty() {
        return None;
    }
    let release = s(k, "ReleaseType").to_lowercase();
    if !s(k, "ParentKeyName").is_empty() || ["update", "hotfix", "security update", "service pack"].contains(&release.as_str()) {
        return None;
    }
    let system_component = d(k, "SystemComponent") == Some(1);
    let msi = d(k, "WindowsInstaller") == Some(1);
    // Installer sub-components (24 of them for one Python): the parent entry covers them.
    if system_component && msi {
        return None;
    }
    let mut install_location = Some(unquote(&s(k, "InstallLocation"))).filter(|p| !p.is_empty()).map(PathBuf::from);
    if install_location.is_none() {
        // Fall back to the folder of the program's own icon, if that is a real program folder.
        install_location = icon_path(&s(k, "DisplayIcon"))
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .filter(|p| p.is_dir() && !p.to_string_lossy().to_lowercase().contains("package cache"));
    }
    let uninstall = Some(normalize_uninstall(&s(k, "UninstallString"))).filter(|c| !c.is_empty());
    let quiet = Some(normalize_uninstall(&s(k, "QuietUninstallString"))).filter(|c| !c.is_empty());
    let no_remove = d(k, "NoRemove") == Some(1);
    let removable = if no_remove {
        Removability::Blocked(crate::tr!(
            "Üretici kaldırmayı engelliyor; zorla kaldırma ile silinebilir".to_string(),
            "The vendor blocks removal; can be removed by force".to_string()
        ))
    } else if uninstall.is_none() {
        Removability::Blocked(crate::tr!(
            "Kaldırıcı yok; zorla kaldırma ile silinebilir".to_string(),
            "No uninstaller; can be removed by force".to_string()
        ))
    } else {
        Removability::Normal
    };
    Some(InstalledApp {
        id: format!("win:{}:{}\\{}", hive_name(hive), base, sub),
        name,
        publisher: s(k, "Publisher"),
        version: s(k, "DisplayVersion"),
        kind: AppKind::Win32,
        install_location,
        size_bytes: d(k, "EstimatedSize").map(|kb| kb as u64 * 1024).unwrap_or(0),
        install_date: parse_install_date(&s(k, "InstallDate")),
        last_used: None,
        uninstall_cmd: uninstall,
        quiet_uninstall_cmd: quiet,
        removable,
        reg: Some(RegRef { hive, path: format!("{base}\\{sub}") }),
        package_id: None,
        system_component,
        icon_exe: icon_path(&s(k, "DisplayIcon"))
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
            .filter(|n| n.ends_with(".exe")),
    })
}

/// Everything listed in "Apps & features", plus (optionally) the Store packages.
pub fn inventory(include_store: bool) -> Vec<InstalledApp> {
    let mut apps: Vec<InstalledApp> = Vec::new();
    for (hive, base) in [(Hive::Machine, UNINSTALL), (Hive::Machine, UNINSTALL32), (Hive::User, UNINSTALL)] {
        let Ok(root) = predef(hive).open_subkey_with_flags(base, KEY_READ) else { continue };
        for sub in root.enum_keys().flatten() {
            if let Ok(k) = root.open_subkey_with_flags(&sub, KEY_READ) {
                if let Some(a) = read_entry(hive, base, &sub, &k) {
                    apps.push(a);
                }
            }
        }
    }
    // The same program often appears in both registry views.
    let mut seen: HashMap<(String, String, String), usize> = HashMap::new();
    let mut out: Vec<InstalledApp> = Vec::new();
    for a in apps {
        let key = (a.name.to_lowercase(), a.version.clone(), a.publisher.to_lowercase());
        match seen.get(&key) {
            Some(&i) => {
                if out[i].uninstall_cmd.is_none() && a.uninstall_cmd.is_some() {
                    out[i] = a;
                }
            }
            None => {
                seen.insert(key, out.len());
                out.push(a);
            }
        }
    }
    if include_store {
        out.extend(store_apps());
    }
    attach_usage(&mut out);
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Fills `size_bytes` for programs whose installer did not report a size (walks the install folder).
pub fn fill_missing_sizes(apps: &mut [InstalledApp], cancel: &AtomicBool) {
    for a in apps.iter_mut().filter(|a| a.size_bytes == 0) {
        if let Some(loc) = a.install_location.as_deref().filter(|p| p.is_dir()) {
            a.size_bytes = dir_stats(loc, cancel).size;
        }
    }
}

fn powershell_json(script: &str) -> Option<serde_json::Value> {
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    hide(&mut c);
    let out = c.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim().trim_start_matches('\u{feff}');
    serde_json::from_str(text).ok()
}

fn json_items(v: serde_json::Value) -> Vec<serde_json::Value> {
    match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Null => Vec::new(),
        other => vec![other],
    }
}

/// `CN=Microsoft Corporation, O=…` → `Microsoft Corporation`
fn cn(dn: &str) -> String {
    dn.split(',')
        .find_map(|p| p.trim().strip_prefix("CN="))
        .unwrap_or(dn)
        .trim_matches('"')
        .to_string()
}

/// Start-menu names of Store apps: `microsoft.windows.photos_8wekyb3d8bbwe` → `Photos`.
fn store_display_names() -> HashMap<String, String> {
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress";
    let mut m = HashMap::new();
    if let Some(v) = powershell_json(script) {
        for it in json_items(v) {
            let (Some(name), Some(id)) = (it["Name"].as_str(), it["AppID"].as_str()) else { continue };
            if let Some((fam, _)) = id.split_once('!') {
                m.entry(fam.to_lowercase()).or_insert_with(|| name.to_string());
            }
        }
    }
    m
}

fn store_apps() -> Vec<InstalledApp> {
    let names = store_display_names();
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
        Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.IsResourcePackage } | \
        Select-Object Name,PackageFullName,Publisher,Version,InstallLocation,NonRemovable,SignatureKind | \
        ConvertTo-Json -Compress";
    let Some(v) = powershell_json(script) else { return Vec::new() };
    json_items(v)
        .into_iter()
        .filter_map(|p| {
            let full = p["PackageFullName"].as_str()?.to_string();
            let raw_name = p["Name"].as_str()?.to_string();
            let name = package_family(&full).and_then(|f| names.get(&f).cloned()).unwrap_or(raw_name);
            let loc = p["InstallLocation"].as_str().filter(|l| !l.is_empty()).map(PathBuf::from);
            let sealed = p["NonRemovable"].as_bool().unwrap_or(false) || p["SignatureKind"].as_str() == Some("System");
            Some(InstalledApp {
                id: format!("store:{full}"),
                name,
                publisher: cn(p["Publisher"].as_str().unwrap_or_default()),
                version: p["Version"].as_str().unwrap_or_default().to_string(),
                kind: AppKind::Store,
                install_location: loc,
                size_bytes: 0,
                install_date: None,
                last_used: None,
                uninstall_cmd: None,
                quiet_uninstall_cmd: None,
                removable: if sealed {
                    Removability::Sealed(crate::tr!(
                        "Windows'a ait korumalı paket; hiçbir uygulama kaldıramaz".to_string(),
                        "Protected Windows package; no program can remove it".to_string()
                    ))
                } else {
                    Removability::Normal
                },
                reg: None,
                package_id: Some(full),
                system_component: false,
                icon_exe: None,
            })
        })
        .collect()
}

// ---- last-use guess (UserAssist) -------------------------------------------------------------

/// Known-folder GUIDs that UserAssist writes instead of the real folder.
fn known_folder(guid: &str) -> Option<String> {
    let env = |k: &str| std::env::var(k).ok();
    let home = crate::config::home_dir().to_string_lossy().into_owned();
    Some(match guid.to_uppercase().as_str() {
        "{6D809377-6AF0-444B-8957-A3773F02200E}" => env("ProgramW6432").unwrap_or_else(|| r"C:\Program Files".into()),
        "{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}" => env("ProgramFiles(x86)").unwrap_or_else(|| r"C:\Program Files (x86)".into()),
        "{905E63B6-C1BF-494E-B29C-65B732D3D21A}" => env("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()),
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}" => format!(r"{}\System32", env("SystemRoot")?),
        "{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}" => format!(r"{}\SysWOW64", env("SystemRoot")?),
        "{F38BF404-1D43-42F2-9305-67DE0B28FC23}" => env("SystemRoot")?,
        "{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}" => env("LOCALAPPDATA")?,
        "{3EB685DB-65F9-4CF6-A03A-E3EF65729F3D}" => env("APPDATA")?,
        "{62AB5D82-FDC1-4DC3-A9DD-070D1D495D97}" => env("ProgramData")?,
        "{5E6C858F-0E22-4760-9AFE-EA3317B67173}" => home,
        "{B4BFCC3A-DB2C-424C-B029-7FE99A87C641}" => format!(r"{home}\Desktop"),
        "{374DE290-123F-4565-9164-39C4925E467B}" => format!(r"{home}\Downloads"),
        "{FDD39AD0-238F-46AF-ADB4-6C85480369C7}" => format!(r"{home}\Documents"),
        _ => return None,
    })
}

/// Decoded UserAssist entries: (lower-case path or AppUserModelID, last run as unix seconds).
pub fn user_assist() -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist", KEY_READ)
    else {
        return out;
    };
    for g in root.enum_keys().flatten() {
        let Ok(count) = root.open_subkey_with_flags(format!(r"{g}\Count"), KEY_READ) else { continue };
        for (name, val) in count.enum_values().flatten() {
            if val.bytes.len() < 68 {
                continue;
            }
            let ft = u64::from_le_bytes(val.bytes[60..68].try_into().unwrap());
            let Some(ts) = filetime_to_unix(ft) else { continue };
            let mut path = rot13(&name);
            if path.starts_with('{') {
                if let Some(end) = path.find('}') {
                    if let Some(real) = known_folder(&path[..=end]) {
                        path = format!("{real}{}", &path[end + 1..]);
                    }
                }
            }
            out.push((path.replace('/', "\\").to_lowercase(), ts));
        }
    }
    out
}

/// Store package family: `Name_1.2.3.4_x64__8wekyb3d8bbwe` → `name_8wekyb3d8bbwe`.
fn package_family(full: &str) -> Option<String> {
    let name = full.split('_').next()?;
    let publisher_id = full.rsplit('_').next()?;
    Some(format!("{name}_{publisher_id}").to_lowercase())
}

/// Prefetch: Windows writes `NAME.EXE-HASH.pf` and touches it on every launch, so the file's
/// modified time is the last run. Returns lower-case exe name → latest launch (unix seconds).
pub fn prefetch_runs() -> HashMap<String, i64> {
    let mut m: HashMap<String, i64> = HashMap::new();
    let dir = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("Prefetch");
    let Ok(rd) = std::fs::read_dir(dir) else { return m };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let Some(stem) = name.strip_suffix(".pf") else { continue };
        let Some((exe, _hash)) = stem.rsplit_once('-') else { continue };
        let Some(t) = e.metadata().ok().and_then(|md| md.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) else { continue };
        let t = t.as_secs() as i64;
        let slot = m.entry(exe.to_string()).or_insert(0);
        *slot = (*slot).max(t);
    }
    m
}

/// Background Activity Moderator: last launch time of every executable, per user. Reading it
/// needs rights that a normal account may lack; then this is simply empty.
pub fn bam_runs() -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\bam\State\UserSettings", KEY_READ) else {
        return out;
    };
    for sid in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey_with_flags(&sid, KEY_READ) else { continue };
        for (name, val) in k.enum_values().flatten() {
            if val.bytes.len() < 8 {
                continue;
            }
            let Some(ts) = filetime_to_unix(u64::from_le_bytes(val.bytes[..8].try_into().unwrap())) else { continue };
            // `\Device\HarddiskVolume3\Program Files\X\x.exe` → `\program files\x\x.exe`
            let p = name.to_lowercase();
            let p = p
                .strip_prefix(r"\device\")
                .and_then(|r| r.split_once('\\'))
                .map(|(_, rest)| format!("\\{rest}"))
                .unwrap_or(p);
            out.push((p, ts));
        }
    }
    out
}

/// Executable names in the program's folder (top level + one level down), for Prefetch matching.
fn exe_names(a: &InstalledApp) -> Vec<String> {
    let mut v: Vec<String> = a.icon_exe.iter().filter(|n| !is_background_exe(n)).cloned().collect();
    if let Some(loc) = a.install_location.as_deref().filter(|p| p.is_dir()) {
        let mut budget = 400;
        let mut stack = vec![(loc.to_path_buf(), 0)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                let p = e.path();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() && !ft.is_symlink() && depth < 1 {
                    stack.push((p, depth + 1));
                } else if p.extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false) {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if !is_background_exe(&n) {
                        v.push(n);
                    }
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

/// Best guess of the last time each program was used: the newest of UserAssist (programs started
/// from Start/Explorer, also by shortcut or AppUserModelID name), Prefetch and BAM.
pub fn attach_usage(apps: &mut [InstalledApp]) {
    let ua = user_assist();
    let prefetch = prefetch_runs();
    let bam = bam_runs();
    for a in apps.iter_mut() {
        let mut best: Option<i64> = None;
        let mut take = |t: Option<i64>| best = best.max(t);
        match a.kind {
            AppKind::Win32 => {
                if let Some(loc) = a.install_location.as_ref() {
                    let l = loc.to_string_lossy().replace('/', "\\").to_lowercase();
                    let l = l.trim_end_matches('\\').to_string();
                    if l.len() >= 8 {
                        let fg = |p: &str| !is_background_exe(p.rsplit('\\').next().unwrap_or(p));
                        take(ua.iter().filter(|(p, _)| p.starts_with(&format!("{l}\\")) && fg(p)).map(|(_, t)| *t).max());
                        let no_drive = l.split_once(':').map(|(_, r)| r.to_string()).unwrap_or_else(|| l.clone());
                        take(bam.iter().filter(|(p, _)| p.contains(&format!("{no_drive}\\")) && fg(p)).map(|(_, t)| *t).max());
                    }
                }
                for exe in exe_names(a) {
                    take(prefetch.get(&exe).copied());
                }
                // Started through a shortcut / AppUserModelID named after the program.
                let keys = nested_variants(&a.name, &a.publisher);
                if !keys.is_empty() {
                    take(
                        ua.iter()
                            .filter(|(p, _)| {
                                let tail = p.rsplit('\\').next().unwrap_or(p);
                                let stem = tail.strip_suffix(".lnk").unwrap_or(tail);
                                let is_name_like = p.ends_with(".lnk") || !p.contains('\\');
                                is_name_like && keys.contains(&norm_key(stem.split('!').next().unwrap_or(stem)))
                            })
                            .map(|(_, t)| *t)
                            .max(),
                    );
                }
            }
            AppKind::Store => {
                if let Some(fam) = a.package_id.as_deref().and_then(package_family) {
                    take(ua.iter().filter(|(p, _)| p.starts_with(&fam)).map(|(_, t)| *t).max());
                }
            }
            AppKind::MacApp => {}
        }
        a.last_used = best;
    }
}

// ---- running the uninstaller ----------------------------------------------------------------

/// Runs the vendor's uninstaller and waits for it. The system shows its own administrator
/// prompt when the uninstaller needs one. Returns the exit code.
/// `force`: add `--force-uninstall` for Chromium-style installers (Edge, WebView2).
pub fn run_uninstaller(app: &InstalledApp, silent: bool, force: bool) -> Result<i32, String> {
    if app.kind == AppKind::Store {
        let full = app.package_id.as_deref().ok_or("no package id")?;
        return remove_store_package(full, crate::core::elevate::is_elevated());
    }
    let mut cmd = match (silent, &app.quiet_uninstall_cmd) {
        (true, Some(q)) => q.clone(),
        _ => app.uninstall_cmd.clone().ok_or_else(|| {
            crate::tr!("Kaldırıcı yok; zorla kaldırmayı kullan".to_string(), "No uninstaller; use forced removal".to_string())
        })?,
    };
    if force {
        if let Some(f) = force_variant(&cmd) {
            cmd = f;
        }
    }
    let (exe, args) = split_command(&cmd);
    let mut c = std::process::Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "$a=@{FilePath=$env:DRAXBYTE_EXE;Wait=$true;PassThru=$true}; \
         if ($env:DRAXBYTE_ARGS) { $a.ArgumentList=$env:DRAXBYTE_ARGS }; \
         $p=Start-Process @a; exit $p.ExitCode",
    ])
    .env("DRAXBYTE_EXE", exe)
    .env("DRAXBYTE_ARGS", args);
    hide(&mut c);
    let out = c.output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(code) => Ok(code),
        None => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

fn remove_store_package(full: &str, elevated: bool) -> Result<i32, String> {
    let script = if elevated {
        "Remove-AppxPackage -Package $env:DRAXBYTE_PKG -AllUsers -ErrorAction Stop"
    } else {
        "Remove-AppxPackage -Package $env:DRAXBYTE_PKG -ErrorAction Stop"
    };
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]).env("DRAXBYTE_PKG", full);
    hide(&mut c);
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(0)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Is the program's own "Uninstall" entry still registered? (Polled while an uninstaller runs
/// in the background, e.g. NSIS/Inno copies that return at once.)
pub fn still_registered(app: &InstalledApp) -> bool {
    match &app.reg {
        Some(r) => predef(r.hive).open_subkey_with_flags(&r.path, KEY_READ).is_ok(),
        None => app.install_location.as_deref().map(|p| p.exists()).unwrap_or(false),
    }
}

/// Force-closes everything still running from the program's folder. Only processes whose
/// executable lies inside `install_location` are touched (never by name).
pub fn close_running(app: &InstalledApp) -> crate::core::memory::CloseReport {
    let Some(loc) = app.install_location.as_deref() else { return Default::default() };
    let pids: Vec<u32> = processes_under(loc).iter().flat_map(|g| g.pids.clone()).collect();
    crate::core::memory::close_pids(&pids, true)
}

// ---- leftovers -------------------------------------------------------------------------------

fn path_size(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => dir_stats(p, &AtomicBool::new(false)).size,
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

struct Finder<'a> {
    app: &'a InstalledApp,
    env: &'a ScanEnv,
    variants: Vec<String>,
    /// Same, plus the name without the publisher's first word ("Google Chrome" → "chrome").
    nested_variants: Vec<String>,
    publisher_words: Vec<String>,
    shared_publisher: bool,
    out: Vec<Leftover>,
    seen: std::collections::HashSet<String>,
}

impl<'a> Finder<'a> {
    fn new(app: &'a InstalledApp, others: &[InstalledApp], env: &'a ScanEnv) -> Self {
        let variants = name_variants(&app.name);
        let pub_norm = norm_key(&app.publisher);
        let first_word = app
            .publisher
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| !w.is_empty())
            .map(norm_key)
            .unwrap_or_default();
        let nested = nested_variants(&app.name, &app.publisher);
        let mut publisher_words = vec![pub_norm.clone()];
        if !first_word.is_empty() {
            publisher_words.push(first_word);
        }
        publisher_words.retain(|w| w.len() >= 3);
        let shared_publisher = !pub_norm.is_empty()
            && others.iter().any(|o| o.id != app.id && norm_key(&o.publisher) == pub_norm);
        Finder { app, env, variants, nested_variants: nested, publisher_words, shared_publisher, out: Vec::new(), seen: Default::default() }
    }

    fn push(&mut self, mut l: Leftover) {
        if matches!(l.kind, LeftoverKind::Path { .. }) {
            l.label = l.label.trim_end_matches(['\\', '/']).to_string();
        }
        if self.seen.insert(l.label.to_lowercase()) {
            self.out.push(l);
        }
    }

    fn is_vendor_folder(&self, name: &str) -> bool {
        let n = norm_key(name);
        self.publisher_words.iter().any(|w| *w == n)
    }

    fn path_item(&mut self, p: &Path, conf: Confidence, why: &str) {
        let loc = self.app.install_location.as_deref();
        if !leftover_path_allowed(p, loc, &self.env.roots) {
            return;
        }
        let Ok(meta) = std::fs::symlink_metadata(p) else { return };
        // Browser/app profiles hold the user's own data: shown, but not pre-selected.
        let user_data = p.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("user data"))
            || (meta.is_dir() && p.join("User Data").is_dir());
        let (conf, why) = if user_data {
            (Confidence::Medium, "may contain your profile (bookmarks, saved passwords, settings)".to_string())
        } else {
            (conf, why.to_string())
        };
        self.push(Leftover {
            kind: LeftoverKind::Path { is_dir: meta.is_dir() },
            label: p.to_string_lossy().into_owned(),
            size: path_size(p),
            confidence: conf,
            why,
        });
    }

    fn scan_dir_roots(&mut self) {
        let r = &self.env.roots;
        let mut bases: Vec<PathBuf> = r.program_files.clone();
        for p in [&r.program_data, &r.local_appdata, &r.roaming_appdata, &r.local_low].into_iter().flatten() {
            bases.push(p.clone());
        }
        for base in bases {
            let Ok(rd) = std::fs::read_dir(&base) else { continue };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if !ft.is_dir() || ft.is_symlink() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let path = e.path();
                if let Some((conf, why)) = match_name(&name, &self.variants, &self.app.publisher, self.shared_publisher) {
                    self.path_item(&path, conf, why);
                }
                // <vendor>\<product>: Google\Chrome, Mozilla\Firefox, JetBrains\Rider…
                if self.is_vendor_folder(&name) {
                    if let Ok(inner) = std::fs::read_dir(&path) {
                        for ie in inner.flatten() {
                            if !ie.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false) {
                                continue;
                            }
                            let iname = ie.file_name().to_string_lossy().into_owned();
                            let n = norm_key(&iname);
                            if self.nested_variants.iter().any(|v| *v == n) {
                                self.path_item(&ie.path(), Confidence::High, "inside the vendor folder, name matches");
                            }
                        }
                    }
                }
            }
        }
    }

    fn scan_shortcuts(&mut self) {
        let r = &self.env.roots;
        let mut dirs: Vec<PathBuf> = r.start_menu.clone();
        dirs.extend(r.desktop.clone());
        for dir in dirs {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let is_lnk = path.extension().map(|x| x.eq_ignore_ascii_case("lnk")).unwrap_or(false);
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if !(is_lnk || is_dir) {
                    continue;
                }
                if let Some((conf, why)) = match_name(&stem, &self.variants, &self.app.publisher, self.shared_publisher) {
                    self.path_item(&path, conf, why);
                }
            }
        }
    }

    fn reg_item(&mut self, hive: Hive, path: &str, conf: Confidence, why: &str) {
        if !reg_key_allowed(hive, path) {
            return;
        }
        self.push(Leftover {
            kind: LeftoverKind::RegKey { hive, path: path.to_string() },
            label: format!("{}\\{}", hive_name(hive), path),
            size: 0,
            confidence: conf,
            why: why.to_string(),
        });
    }

    fn scan_registry(&mut self) {
        let roots = self.env.reg_roots.clone();
        for (hive, base) in roots {
            let Ok(root) = predef(hive).open_subkey_with_flags(&base, KEY_READ) else { continue };
            for child in root.enum_keys().flatten() {
                let path = format!("{base}\\{child}");
                if let Some((conf, why)) = match_name(&child, &self.variants, &self.app.publisher, self.shared_publisher) {
                    self.reg_item(hive, &path, conf, why);
                }
                if self.is_vendor_folder(&child) {
                    if let Ok(k) = root.open_subkey_with_flags(&child, KEY_READ) {
                        for inner in k.enum_keys().flatten() {
                            if self.nested_variants.iter().any(|v| *v == norm_key(&inner)) {
                                self.reg_item(hive, &format!("{path}\\{inner}"), Confidence::High, "inside the vendor key, name matches");
                            }
                        }
                    }
                }
            }
        }
        if let Some(r) = &self.app.reg {
            if predef(r.hive).open_subkey_with_flags(&r.path, KEY_READ).is_ok() {
                let (hive, path) = (r.hive, r.path.clone());
                self.reg_item(hive, &path, Confidence::High, "the program's own uninstall entry");
            }
        }
    }

    /// Things that start the program (Run values, services, scheduled tasks) found by their path.
    fn scan_by_install_path(&mut self) {
        let Some(loc) = self.app.install_location.as_ref() else { return };
        let needle = loc.to_string_lossy().replace('/', "\\").to_lowercase();
        let needle = needle.trim_end_matches('\\').to_string();
        if needle.len() < 10 {
            return;
        }
        for (hive, key) in [(Hive::User, RUN), (Hive::Machine, RUN), (Hive::Machine, RUN32)] {
            let Ok(k) = predef(hive).open_subkey_with_flags(key, KEY_READ) else { continue };
            for (name, val) in k.enum_values().flatten() {
                let data = String::from_utf16_lossy(
                    &val.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect::<Vec<u16>>(),
                )
                .replace('/', "\\")
                .to_lowercase();
                if data.contains(&needle) {
                    self.push(Leftover {
                        kind: LeftoverKind::RunValue { hive, key: key.to_string(), name: name.clone() },
                        label: format!("{}\\{} → {}", hive_name(hive), key, name),
                        size: 0,
                        confidence: Confidence::High,
                        why: "starts the program at logon".into(),
                    });
                }
            }
        }
        if self.env.services {
            if let Ok(svcs) = predef(Hive::Machine).open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services", KEY_READ) {
                for name in svcs.enum_keys().flatten() {
                    let Ok(k) = svcs.open_subkey_with_flags(&name, KEY_READ) else { continue };
                    let image = s(&k, "ImagePath").replace('/', "\\").to_lowercase();
                    if image.contains(&needle) {
                        self.push(Leftover {
                            kind: LeftoverKind::Service(name.clone()),
                            label: format!("service {name}"),
                            size: 0,
                            confidence: Confidence::High,
                            why: "Windows service running from the program's folder".into(),
                        });
                    }
                }
            }
        }
        if self.env.tasks {
            let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                Get-ScheduledTask | ForEach-Object { [pscustomobject]@{ Name=$_.TaskName; Path=$_.TaskPath; \
                Exec=(($_.Actions | ForEach-Object { $_.Execute + ' ' + $_.Arguments }) -join ';') } } | ConvertTo-Json -Compress";
            if let Some(v) = powershell_json(script) {
                for t in json_items(v) {
                    let exec = t["Exec"].as_str().unwrap_or_default().replace('/', "\\").to_lowercase();
                    if exec.contains(&needle) {
                        let path = t["Path"].as_str().unwrap_or("\\");
                        let name = t["Name"].as_str().unwrap_or_default();
                        self.push(Leftover {
                            kind: LeftoverKind::Task(format!("{path}{name}")),
                            label: format!("task {path}{name}"),
                            size: 0,
                            confidence: Confidence::High,
                            why: "scheduled task running the program".into(),
                        });
                    }
                }
            }
        }
    }

    /// Edge ships its updater as a separate component with its own folder, services and tasks.
    fn scan_vendor_extras(&mut self) {
        let is_edge = self.app.name.eq_ignore_ascii_case("Microsoft Edge")
            && self.app.publisher.to_lowercase().contains("microsoft");
        if !is_edge {
            return;
        }
        for pf in self.env.roots.program_files.clone() {
            let p = pf.join("Microsoft").join("EdgeUpdate");
            if p.is_dir() {
                self.path_item(&p, Confidence::High, "Edge's updater component");
            }
        }
        if self.env.services {
            for name in ["edgeupdate", "edgeupdatem"] {
                let key = format!(r"SYSTEM\CurrentControlSet\Services\{name}");
                if predef(Hive::Machine).open_subkey_with_flags(&key, KEY_READ).is_ok() {
                    self.push(Leftover {
                        kind: LeftoverKind::Service(name.to_string()),
                        label: format!("service {name}"),
                        size: 0,
                        confidence: Confidence::High,
                        why: "Edge's update service".into(),
                    });
                }
            }
        }
        if self.env.tasks {
            let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
                Get-ScheduledTask -TaskName 'MicrosoftEdgeUpdate*' -ErrorAction SilentlyContinue | \
                Select-Object @{n='Name';e={$_.TaskName}},@{n='Path';e={$_.TaskPath}} | ConvertTo-Json -Compress";
            if let Some(v) = powershell_json(script) {
                for t in json_items(v) {
                    let path = t["Path"].as_str().unwrap_or("\\");
                    let name = t["Name"].as_str().unwrap_or_default();
                    if !name.is_empty() {
                        self.push(Leftover {
                            kind: LeftoverKind::Task(format!("{path}{name}")),
                            label: format!("task {path}{name}"),
                            size: 0,
                            confidence: Confidence::High,
                            why: "Edge's update task".into(),
                        });
                    }
                }
            }
        }
    }
}

/// Everything that still belongs to `app`: its folders, shortcuts, registry keys, autostart
/// entries, services and scheduled tasks. Works before (to preview) and after the uninstaller.
/// `others` = the rest of the installed programs (to avoid claiming a vendor folder they share).
pub fn find_leftovers(app: &InstalledApp, others: &[InstalledApp], env: &ScanEnv) -> Vec<Leftover> {
    let mut f = Finder::new(app, others, env);
    if let Some(loc) = app.install_location.clone().filter(|p| p.exists()) {
        f.path_item(&loc, Confidence::High, "the program's install folder");
    }
    f.scan_dir_roots();
    f.scan_shortcuts();
    f.scan_registry();
    f.scan_by_install_path();
    f.scan_vendor_extras();
    let mut v = prune_nested(f.out);
    v.sort_by(|a, b| a.confidence.cmp(&b.confidence).then(b.size.cmp(&a.size)));
    v
}

/// A folder inside another listed folder of the same or better confidence is redundant: removing
/// the outer one removes it (and its size would be counted twice).
fn prune_nested(items: Vec<Leftover>) -> Vec<Leftover> {
    let paths: Vec<(usize, PathBuf, Confidence)> = items
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l.kind, LeftoverKind::Path { is_dir: true }))
        .map(|(i, l)| (i, PathBuf::from(&l.label), l.confidence))
        .collect();
    items
        .into_iter()
        .enumerate()
        .filter(|(i, l)| {
            if !matches!(l.kind, LeftoverKind::Path { .. }) {
                return true;
            }
            let me = PathBuf::from(&l.label);
            !paths.iter().any(|(j, p, c)| j != i && *c <= l.confidence && me != *p && is_under(&me, p))
        })
        .map(|(_, l)| l)
        .collect()
}

// ---- removal ---------------------------------------------------------------------------------

fn clear_readonly(path: &Path) {
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if let Ok(m) = std::fs::symlink_metadata(&p) {
                let mut perm = m.permissions();
                if perm.readonly() {
                    perm.set_readonly(false);
                    let _ = std::fs::set_permissions(&p, perm);
                }
                if m.is_dir() && !m.file_type().is_symlink() {
                    clear_readonly(&p);
                }
            }
        }
    }
}

fn delete_path(p: &Path, permanent: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    let is_dir = meta.is_dir() && !meta.file_type().is_symlink();
    if !permanent {
        return trash::delete(p).map_err(|e| e.to_string());
    }
    let rm = || if is_dir { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
    match rm() {
        Ok(()) => Ok(()),
        Err(_) => {
            // Installers leave read-only files behind; clear the flag and retry once.
            if is_dir {
                clear_readonly(p);
            } else if let Ok(m) = std::fs::metadata(p) {
                let mut perm = m.permissions();
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(p, perm);
            }
            rm().map_err(|e| format!("{e} ({})", crate::tr!("kilitli olabilir: ilgili programı kapat", "may be locked: close the program")))
        }
    }
}

fn reg_export(hive: Hive, path: &str, file: &Path) -> Result<(), String> {
    let mut c = std::process::Command::new("reg.exe");
    c.args(["export", &format!("{}\\{}", hive_name(hive), path), &file.to_string_lossy(), "/y"]);
    hide(&mut c);
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stdout).trim().to_string() + &String::from_utf8_lossy(&out.stderr))
    }
}

/// Deletes the chosen leftovers. Every item is re-checked against the guards first, registry
/// keys are exported to `.reg` files before they are removed (and skipped if the export fails),
/// and each failure is reported without stopping the rest.
pub fn remove_leftovers(app: &InstalledApp, items: &[Leftover], opts: &RemoveOptions, env: &ScanEnv) -> RemoveReport {
    let mut rep = RemoveReport { backup_dir: opts.backup_dir.clone(), ..Default::default() };
    let _ = std::fs::create_dir_all(&opts.backup_dir);
    let mut log = String::new();
    let loc = app.install_location.as_deref();
    for (i, it) in items.iter().enumerate() {
        let backup = opts.backup_dir.join(format!("{i:03}.reg"));
        let res: Result<(), String> = match &it.kind {
            LeftoverKind::Path { .. } => {
                let p = Path::new(&it.label);
                if !leftover_path_allowed(p, loc, &env.roots) {
                    Err(crate::tr!("korumalı yol, atlandı".to_string(), "protected path, skipped".to_string()))
                } else if !p.exists() && std::fs::symlink_metadata(p).is_err() {
                    Ok(())
                } else {
                    delete_path(p, opts.permanent).map(|_| rep.freed += it.size)
                }
            }
            LeftoverKind::RegKey { hive, path } => {
                if !reg_key_allowed(*hive, path) {
                    Err("protected registry key, skipped".to_string())
                } else if predef(*hive).open_subkey_with_flags(path, KEY_READ).is_err() {
                    Ok(())
                } else {
                    reg_export(*hive, path, &backup)
                        .map_err(|e| format!("backup failed, key kept: {e}"))
                        .and_then(|_| predef(*hive).delete_subkey_all(path).map_err(|e| e.to_string()))
                }
            }
            LeftoverKind::RunValue { hive, key, name } => match predef(*hive).open_subkey_with_flags(key, KEY_READ | KEY_SET_VALUE) {
                Err(e) => Err(e.to_string()),
                Ok(k) => reg_export(*hive, key, &backup)
                    .map_err(|e| format!("backup failed, value kept: {e}"))
                    .and_then(|_| k.delete_value(name).map_err(|e| e.to_string())),
            },
            LeftoverKind::Service(name) => {
                let key = format!(r"SYSTEM\CurrentControlSet\Services\{name}");
                if !reg_key_allowed(Hive::Machine, &key) {
                    Err("protected service, skipped".to_string())
                } else {
                    let _ = reg_export(Hive::Machine, &key, &backup);
                    for args in [vec!["stop", name.as_str()], vec!["delete", name.as_str()]] {
                        let mut c = std::process::Command::new("sc.exe");
                        c.args(&args);
                        hide(&mut c);
                        let _ = c.output();
                    }
                    if predef(Hive::Machine).open_subkey_with_flags(&key, KEY_READ).is_err() {
                        Ok(())
                    } else {
                        Err(crate::tr!(
                            "silinmek üzere işaretlendi; yeniden başlatınca kalkar (yönetici gerekebilir)".to_string(),
                            "marked for deletion; it disappears after a restart (may need administrator)".to_string()
                        ))
                    }
                }
            }
            LeftoverKind::Task(full) => {
                let script = "$full=$env:DRAXBYTE_TASK; $i=$full.LastIndexOf('\\'); $path=$full.Substring(0,$i+1); $name=$full.Substring($i+1); \
                    Export-ScheduledTask -TaskName $name -TaskPath $path | Out-File -Encoding utf8 -LiteralPath $env:DRAXBYTE_BACKUP; \
                    Unregister-ScheduledTask -TaskName $name -TaskPath $path -Confirm:$false -ErrorAction Stop";
                let mut c = std::process::Command::new("powershell.exe");
                c.args(["-NoProfile", "-NonInteractive", "-Command", script])
                    .env("DRAXBYTE_TASK", full)
                    .env("DRAXBYTE_BACKUP", opts.backup_dir.join(format!("{i:03}.task.xml")));
                hide(&mut c);
                match c.output() {
                    Ok(o) if o.status.success() => Ok(()),
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
                    Err(e) => Err(e.to_string()),
                }
            }
        };
        log.push_str(&format!("{} {}: {}\n", if res.is_ok() { "OK  " } else { "FAIL" }, it.label, res.as_ref().err().map(String::as_str).unwrap_or("")));
        rep.items.push(ItemResult { label: it.label.clone(), ok: res.is_ok(), message: res.err().unwrap_or_default() });
    }
    let _ = std::fs::write(opts.backup_dir.join("log.txt"), log);
    rep
}

/// Imports every `.reg` file of a backup folder again. Returns how many were restored.
pub fn restore_registry_backup(dir: &Path) -> Result<usize, String> {
    let mut n = 0;
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("reg")).unwrap_or(false) {
            let mut c = std::process::Command::new("reg.exe");
            c.args(["import", &p.to_string_lossy()]);
            hide(&mut c);
            let out = c.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            n += 1;
        }
    }
    Ok(n)
}

/// A System Restore point before the removal. Needs administrator rights and System Protection
/// switched on; Windows allows one per 24 hours by default. A failure is reported, never fatal.
pub fn create_restore_point(description: &str) -> Result<(), String> {
    let mut c = std::process::Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Checkpoint-Computer -Description $env:DRAXBYTE_DESC -RestorePointType APPLICATION_UNINSTALL -ErrorAction Stop",
    ])
    .env("DRAXBYTE_DESC", format!("Draxbyte: {description}"));
    hide(&mut c);
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("failed").trim().to_string())
    }
}

/// Stops Windows Update from putting Edge back after it was removed (machine-wide policy value
/// that Microsoft documents for Edge deployment). Needs administrator rights.
pub fn block_edge_reinstall() -> Result<(), String> {
    let (k, _) = RegKey::predef(HKEY_LOCAL_MACHINE).create_subkey(r"SOFTWARE\Microsoft\EdgeUpdate").map_err(|e| e.to_string())?;
    k.set_value("DoNotUpdateToEdgeWithChromium", &1u32).map_err(|e| e.to_string())
}

/// The registry value Task Manager style raw writes need, exposed for the startup module's tests.
#[allow(dead_code)]
fn raw_dword(v: u32) -> RegValue<'static> {
    RegValue { bytes: v.to_le_bytes().to_vec().into(), vtype: REG_DWORD }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn inventory_reads_this_machine() {
        let apps = inventory(false);
        assert!(!apps.is_empty(), "a Windows machine has programs");
        let mut ids = std::collections::HashSet::new();
        for a in &apps {
            assert!(!a.name.is_empty());
            assert!(ids.insert(a.id.clone()), "duplicate id {}", a.id);
            // MSI sub-components of a bundle are hidden
            assert!(!(a.system_component && a.uninstall_cmd.as_deref().map(|c| c.to_lowercase().contains("msiexec")).unwrap_or(false)));
        }
    }

    #[test]
    fn user_assist_decodes_and_parses() {
        // Real registry, read-only: entries are lower-case paths with sane timestamps.
        for (p, t) in user_assist() {
            assert_eq!(p, p.to_lowercase());
            assert!(t > 631_152_000);
        }
    }

    #[test]
    fn store_family_name() {
        assert_eq!(
            package_family("Microsoft.WindowsCalculator_11.2.0.0_x64__8wekyb3d8bbwe").as_deref(),
            Some("microsoft.windowscalculator_8wekyb3d8bbwe")
        );
    }

    /// A made-up program with folders, shortcuts and registry keys in scratch locations: found,
    /// removed with a registry backup, neighbours untouched, registry restorable.
    #[test]
    fn full_leftover_cycle_on_a_scratch_program() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let mk = |p: &str| {
            let d = t.join(p);
            fs::create_dir_all(&d).unwrap();
            d
        };
        let pf = mk("Program Files");
        let pd = mk("ProgramData");
        let local = mk("AppData/Local");
        let roaming = mk("AppData/Roaming");
        let start = mk("AppData/Roaming/Microsoft/Windows/Start Menu/Programs");
        let desktop = mk("Desktop");

        let install = mk("Program Files/Zorblax Studio");
        fs::write(install.join("zorblax.exe"), vec![0u8; 4096]).unwrap();
        let ro = install.join("readonly.dat");
        fs::write(&ro, b"x").unwrap();
        let mut perm = fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&ro, perm).unwrap();
        mk("ProgramData/Zorblax Studio");
        mk("AppData/Local/ZorblaxStudio");
        mk("AppData/Roaming/Zorblax Labs/Zorblax Studio");
        let neighbour = mk("AppData/Roaming/Zorblax Labs/Other Product");
        fs::write(neighbour.join("keep.txt"), b"keep").unwrap();
        let other_pf = mk("Program Files/Unrelated App");
        fs::write(start.join("Zorblax Studio.lnk"), b"lnk").unwrap();
        fs::write(start.join("Unrelated App.lnk"), b"lnk").unwrap();
        fs::write(desktop.join("Zorblax Studio.lnk"), b"lnk").unwrap();
        fs::write(desktop.join("notes.docx"), b"my notes").unwrap();

        // scratch registry branch standing in for HKCU\Software
        let base = format!(r"Software\DraxbyteTest-apps-{}", std::process::id());
        let soft = format!(r"{base}\Software");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.create_subkey(format!(r"{soft}\Zorblax Labs\Zorblax Studio")).unwrap();
        hkcu.create_subkey(format!(r"{soft}\Zorblax Labs\Other Product")).unwrap();
        hkcu.create_subkey(format!(r"{soft}\Zorblax Studio")).unwrap();

        let env = ScanEnv {
            roots: Roots {
                program_files: vec![pf.clone()],
                program_data: Some(pd.clone()),
                local_appdata: Some(local.clone()),
                roaming_appdata: Some(roaming.clone()),
                local_low: None,
                start_menu: vec![start.clone()],
                desktop: vec![desktop.clone()],
                system_root: None,
            },
            reg_roots: vec![(Hive::User, soft.clone())],
            services: false,
            tasks: false,
        };
        let app = InstalledApp {
            id: "test".into(),
            name: "Zorblax Studio 2.1 (64-bit)".into(),
            publisher: "Zorblax Labs".into(),
            version: "2.1".into(),
            kind: AppKind::Win32,
            install_location: Some(install.clone()),
            size_bytes: 0,
            install_date: None,
            last_used: None,
            uninstall_cmd: None,
            quiet_uninstall_cmd: None,
            removable: Removability::Normal,
            reg: None,
            package_id: None,
            system_component: false,
            icon_exe: None,
        };
        let sibling = InstalledApp { id: "sibling".into(), name: "Other Product".into(), ..app.clone() };

        // Zorblax Labs is shared with another installed program: the vendor folder is not claimed,
        // but the product folders inside it are.
        let found = find_leftovers(&app, &[sibling], &env);
        let labels: Vec<String> = found.iter().map(|l| l.label.clone()).collect();
        let has = |needle: &str| labels.iter().any(|l| l.to_lowercase().replace('/', "\\").contains(&needle.to_lowercase().replace('/', "\\")));
        assert!(has(r"Program Files\Zorblax Studio"), "{labels:?}");
        assert!(has(r"ProgramData\Zorblax Studio"), "{labels:?}");
        assert!(has(r"Local\ZorblaxStudio"), "{labels:?}");
        assert!(has(r"Zorblax Labs\Zorblax Studio"), "{labels:?}");
        assert!(has("Zorblax Studio.lnk"), "{labels:?}");
        assert!(!has("Other Product"), "{labels:?}");
        assert!(!has("Unrelated App"), "{labels:?}");
        assert!(!has("notes.docx"), "{labels:?}");
        assert!(found.iter().any(|l| matches!(&l.kind, LeftoverKind::RegKey { path, .. } if path.ends_with(r"Zorblax Labs\Zorblax Studio"))));
        assert!(found.iter().any(|l| matches!(&l.kind, LeftoverKind::RegKey { path, .. } if path.ends_with(r"Software\Zorblax Studio"))));
        assert!(!found.iter().any(|l| l.label.contains("Other Product")));
        assert!(found.iter().all(|l| l.confidence == Confidence::High), "all exact matches are High");

        let backup = t.join("backup");
        let rep = remove_leftovers(&app, &found, &RemoveOptions { permanent: true, backup_dir: backup.clone() }, &env);
        assert_eq!(rep.failed(), 0, "{:?}", rep.items.iter().filter(|i| !i.ok).collect::<Vec<_>>());
        assert!(!install.exists(), "install folder (with a read-only file) is gone");
        assert!(!pd.join("Zorblax Studio").exists());
        assert!(!local.join("ZorblaxStudio").exists());
        assert!(!roaming.join("Zorblax Labs/Zorblax Studio").exists());
        assert!(!start.join("Zorblax Studio.lnk").exists());
        assert!(!desktop.join("Zorblax Studio.lnk").exists());
        // neighbours untouched
        assert!(neighbour.join("keep.txt").exists());
        assert!(other_pf.exists());
        assert!(start.join("Unrelated App.lnk").exists());
        assert!(desktop.join("notes.docx").exists());
        assert!(hkcu.open_subkey(format!(r"{soft}\Zorblax Labs\Other Product")).is_ok());
        assert!(hkcu.open_subkey(format!(r"{soft}\Zorblax Labs\Zorblax Studio")).is_err());
        assert!(hkcu.open_subkey(format!(r"{soft}\Zorblax Studio")).is_err());
        let regs: Vec<_> = fs::read_dir(&backup).unwrap().flatten().filter(|e| e.path().extension().map(|x| x == "reg").unwrap_or(false)).collect();
        assert_eq!(regs.len(), 2, "one .reg backup per removed key");

        // Undo the registry part.
        assert_eq!(restore_registry_backup(&backup).unwrap(), 2);
        assert!(hkcu.open_subkey(format!(r"{soft}\Zorblax Studio")).is_ok());
        assert!(hkcu.open_subkey(format!(r"{soft}\Zorblax Labs\Zorblax Studio")).is_ok());

        hkcu.delete_subkey_all(&base).unwrap();
    }

    #[test]
    fn guards_are_applied_again_at_removal_time() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("Documents");
        fs::create_dir_all(&victim).unwrap();
        fs::write(victim.join("thesis.docx"), b"important").unwrap();
        let env = ScanEnv {
            roots: Roots { program_files: vec![tmp.path().join("Program Files")], ..Default::default() },
            reg_roots: vec![],
            services: false,
            tasks: false,
        };
        let app = InstalledApp {
            id: "t".into(), name: "Anything".into(), publisher: String::new(), version: String::new(), kind: AppKind::Win32,
            install_location: None, size_bytes: 0, install_date: None, last_used: None, uninstall_cmd: None,
            quiet_uninstall_cmd: None, removable: Removability::Normal, reg: None, package_id: None, system_component: false, icon_exe: None,
        };
        let evil = vec![
            Leftover { kind: LeftoverKind::Path { is_dir: true }, label: victim.to_string_lossy().into(), size: 0, confidence: Confidence::High, why: String::new() },
            Leftover { kind: LeftoverKind::RegKey { hive: Hive::Machine, path: r"SOFTWARE\Microsoft\Windows NT\CurrentVersion".into() }, label: "x".into(), size: 0, confidence: Confidence::High, why: String::new() },
        ];
        let rep = remove_leftovers(&app, &evil, &RemoveOptions { permanent: true, backup_dir: tmp.path().join("b") }, &env);
        assert_eq!(rep.failed(), 2);
        assert!(victim.join("thesis.docx").exists());
    }

    fn scratch_app(name: &str, publisher: &str, loc: &Path) -> InstalledApp {
        InstalledApp {
            id: name.into(), name: name.into(), publisher: publisher.into(), version: String::new(), kind: AppKind::Win32,
            install_location: Some(loc.to_path_buf()), size_bytes: 0, install_date: None, last_used: None, uninstall_cmd: None,
            quiet_uninstall_cmd: None, removable: Removability::Normal, reg: None, package_id: None, system_component: false,
            icon_exe: None,
        }
    }

    fn scratch_env(t: &Path) -> (ScanEnv, PathBuf, PathBuf) {
        let pf = t.join("Program Files");
        let local = t.join("AppData/Local");
        for d in [&pf, &local, &t.join("AppData/Roaming"), &t.join("ProgramData")] {
            fs::create_dir_all(d).unwrap();
        }
        let env = ScanEnv {
            roots: Roots {
                program_files: vec![pf.clone()],
                program_data: Some(t.join("ProgramData")),
                local_appdata: Some(local.clone()),
                roaming_appdata: Some(t.join("AppData/Roaming")),
                ..Default::default()
            },
            reg_roots: vec![],
            services: false,
            tasks: false,
        };
        (env, pf, local)
    }

    #[test]
    fn browser_profile_is_probable_and_not_preselected() {
        let tmp = tempfile::tempdir().unwrap();
        let (env, pf, local) = scratch_env(tmp.path());
        let install = pf.join("Zorb Browser/Application");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(local.join("Zorb/Browser/User Data/Default")).unwrap();
        fs::write(local.join("Zorb/Browser/User Data/Default/Bookmarks"), b"my bookmarks").unwrap();
        let app = scratch_app("Zorb Browser", "Zorb LLC", &install);
        let found = find_leftovers(&app, &[], &env);
        let profile = found.iter().find(|l| l.label.ends_with(r"Zorb\Browser") || l.label.ends_with("Zorb/Browser")).expect("profile folder listed");
        assert_eq!(profile.confidence, Confidence::Medium, "{:?}", found.iter().map(|l| (&l.label, l.confidence)).collect::<Vec<_>>());
        assert!(profile.why.contains("profile"));
    }

    #[test]
    fn folder_inside_a_listed_folder_is_not_listed_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let (env, pf, _) = scratch_env(tmp.path());
        let install = pf.join("Zorb Tool");
        fs::create_dir_all(install.join("bin")).unwrap();
        let app = scratch_app("Zorb Tool", "Zorb LLC", &install);
        let mut with_slash = install.clone().into_os_string();
        with_slash.push("\\");
        let app = InstalledApp { install_location: Some(PathBuf::from(with_slash)), ..app };
        let found = find_leftovers(&app, &[], &env);
        let paths: Vec<_> = found.iter().filter(|l| matches!(l.kind, LeftoverKind::Path { .. })).collect();
        assert_eq!(paths.len(), 1, "{:?}", paths.iter().map(|l| &l.label).collect::<Vec<_>>());
    }
}
