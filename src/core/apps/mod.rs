//! Installed programs: list them, guess when they were last used, remove them completely
//! (the vendor's uninstaller first, then the leftovers) or by force when there is no uninstaller.
//!
//! This file is the platform-independent part: the data model, how leftovers are matched to a
//! program, and the guards that decide which paths / registry keys may ever be deleted.
//! The Windows implementation is in `win.rs`, the macOS one in `mac.rs`.

use std::path::{Component, Path, PathBuf};

#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(windows)]
pub mod win;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppKind {
    /// Classic installer entry (registry "Uninstall" key).
    Win32,
    /// Microsoft Store / MSIX package.
    Store,
    /// `.app` bundle.
    MacApp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removability {
    Normal,
    /// The vendor hides or blocks the uninstaller (Edge, "NoRemove"). Forced removal still works.
    Blocked(String),
    /// No program can remove it: Windows-protected packages, macOS apps on the sealed system volume.
    Sealed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    User,
    Machine,
}

/// Where a program's "Uninstall" entry lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegRef {
    pub hive: Hive,
    /// Path below the hive, e.g. `Software\Microsoft\Windows\CurrentVersion\Uninstall\Foo`.
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct InstalledApp {
    /// Stable identifier (registry key, package full name or bundle path).
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub kind: AppKind,
    pub install_location: Option<PathBuf>,
    pub size_bytes: u64,
    /// Unix seconds.
    pub install_date: Option<i64>,
    /// Unix seconds; `None` = Windows/macOS has no record (services, command-line tools and
    /// Store apps are often invisible to it).
    pub last_used: Option<i64>,
    pub uninstall_cmd: Option<String>,
    pub quiet_uninstall_cmd: Option<String>,
    pub removable: Removability,
    pub reg: Option<RegRef>,
    /// Store package full name / macOS bundle identifier.
    pub package_id: Option<String>,
    /// Hidden by its installer from "Apps & features" (runtimes such as WebView2). Still removable.
    pub system_component: bool,
    /// File name of the program's main executable (from its icon entry), lower case.
    pub icon_exe: Option<String>,
}

impl InstalledApp {
    /// Days since last use; `None` when unknown.
    pub fn days_unused(&self, now: i64) -> Option<i64> {
        self.last_used.map(|t| ((now - t).max(0)) / 86_400)
    }
}

// ---- leftovers -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Exact name/path match; pre-selected.
    High,
    /// Probable (shared vendor folder, partial name); shown, not pre-selected.
    Medium,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeftoverKind {
    Path { is_dir: bool },
    RegKey { hive: Hive, path: String },
    /// A value under a Run key that starts the program at logon.
    RunValue { hive: Hive, key: String, name: String },
    Service(String),
    Task(String),
}

#[derive(Debug, Clone)]
pub struct Leftover {
    pub kind: LeftoverKind,
    /// What the user sees (path, registry path, service name…).
    pub label: String,
    pub size: u64,
    pub confidence: Confidence,
    pub why: String,
}

/// Lower-case letters and digits only: "Mozilla Firefox (x64 en-US)" and "mozilla-firefox" compare alike.
pub fn norm_key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn is_version_token(t: &str) -> bool {
    let t = t.trim_start_matches(['v', 'V']);
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || c == '.') && t.chars().any(|c| c.is_ascii_digit())
}

/// Normalised spellings a program's folders/keys may use: the full display name and the name
/// without architecture/version noise ("Foo 2024 (64-bit) 5.1.2" → "foo"). Short names (< 4
/// characters) are dropped: matching "Go" or "R" would hit unrelated folders.
pub fn name_variants(name: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    v.push(norm_key(name));
    let mut stripped = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => stripped.push(ch),
            _ => {}
        }
    }
    let noise = ["x64", "x86", "64bit", "32bit", "64-bit", "32-bit", "amd64", "arm64", "win64", "win32"];
    let kept: Vec<&str> = stripped
        .split_whitespace()
        .filter(|t| !is_version_token(t) && !noise.contains(&t.to_lowercase().as_str()))
        .collect();
    v.push(norm_key(&kept.join(" ")));
    // A trailing year/edition number is noise too ("Visual Studio Code 1.9" handled above; "Foo 2024").
    if let Some((last, rest)) = kept.split_last() {
        if last.len() == 4 && last.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() {
            v.push(norm_key(&rest.join(" ")));
        }
    }
    v.retain(|s| s.chars().count() >= 4);
    v.sort();
    v.dedup();
    v
}

/// `name_variants` plus the name without the publisher's first word ("Google Chrome" by
/// "Google LLC" → "chrome"): how a vendor folder or a shortcut often spells the product.
pub fn nested_variants(name: &str, publisher: &str) -> Vec<String> {
    let variants = name_variants(name);
    let first_word = publisher
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .map(norm_key)
        .unwrap_or_default();
    let mut nested = variants.clone();
    for v in &variants {
        if first_word.len() >= 3 && v.starts_with(&first_word) && v.chars().count() - first_word.chars().count() >= 4 {
            nested.push(v[first_word.len()..].to_string());
        }
    }
    nested.sort();
    nested.dedup();
    nested
}

/// Vendor names so widely shared that a folder carrying only that name never proves a leftover.
const SHARED_VENDORS: &[&str] = &[
    "microsoft", "microsoftcorporation", "google", "googlellc", "adobe", "adobeinc", "apple", "appleinc",
    "intel", "intelcorporation", "nvidia", "nvidiacorporation", "amd", "advancedmicrodevices", "oracle",
    "oraclecorporation", "python", "pythonsoftwarefoundation", "jetbrains", "mozilla", "mozillafoundation",
    "windows", "common", "commonfiles", "packages", "programs", "temp", "cache", "logs", "data", "apps",
];

/// How a folder / key name relates to a program, if at all.
/// `shared_publisher`: another installed program has the same publisher, so a folder named after
/// the publisher may belong to that one.
pub fn match_name(
    entry_name: &str,
    variants: &[String],
    publisher: &str,
    shared_publisher: bool,
) -> Option<(Confidence, &'static str)> {
    let e = norm_key(entry_name);
    if e.chars().count() < 3 {
        return None;
    }
    if variants.iter().any(|v| *v == e) {
        return Some((Confidence::High, "name matches the program"));
    }
    let p = norm_key(publisher);
    if !p.is_empty() && e == p && !shared_publisher && !SHARED_VENDORS.contains(&p.as_str()) {
        return Some((Confidence::Medium, "named after the publisher; no other installed program shares it"));
    }
    // "<publisher> <name>" folders: "Mozilla Firefox", "Adobe Reader".
    if !p.is_empty() && variants.iter().any(|v| e == format!("{p}{v}") || e == format!("{v}{p}")) {
        return Some((Confidence::High, "publisher + name matches the program"));
    }
    None
}

// ---- removal report -------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RemoveOptions {
    /// Skip the Recycle Bin (frees the disk at once).
    pub permanent: bool,
    /// Registry exports and the action log go here.
    pub backup_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ItemResult {
    pub label: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct RemoveReport {
    pub items: Vec<ItemResult>,
    pub freed: u64,
    pub backup_dir: PathBuf,
}

impl RemoveReport {
    pub fn failed(&self) -> usize {
        self.items.iter().filter(|i| !i.ok).count()
    }
}

// ---- guards ---------------------------------------------------------------------------------

/// Folder roots on this machine that program remnants may live in.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub program_files: Vec<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub local_appdata: Option<PathBuf>,
    pub roaming_appdata: Option<PathBuf>,
    pub local_low: Option<PathBuf>,
    pub start_menu: Vec<PathBuf>,
    pub desktop: Vec<PathBuf>,
    pub system_root: Option<PathBuf>,
}

impl Roots {
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let mut program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|k| env(k))
            .collect();
        program_files.sort();
        program_files.dedup();
        let local = env("LOCALAPPDATA");
        let roaming = env("APPDATA");
        let pd = env("ProgramData");
        let home = crate::config::home_dir();
        Roots {
            program_files,
            local_low: local.as_ref().and_then(|l| l.parent().map(|p| p.join("LocalLow"))),
            start_menu: [
                roaming.as_ref().map(|r| r.join(r"Microsoft\Windows\Start Menu\Programs")),
                pd.as_ref().map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            desktop: [Some(home.join("Desktop")), env("PUBLIC").map(|p| p.join("Desktop"))]
                .into_iter()
                .flatten()
                .collect(),
            local_appdata: local,
            roaming_appdata: roaming,
            program_data: pd,
            system_root: env("SystemRoot"),
        }
    }

    fn container_roots(&self) -> Vec<&Path> {
        let mut v: Vec<&Path> = self.program_files.iter().map(|p| p.as_path()).collect();
        for p in [&self.program_data, &self.local_appdata, &self.roaming_appdata, &self.local_low].into_iter().flatten() {
            v.push(p.as_path());
        }
        v
    }
}

fn comps_lc(p: &Path) -> Vec<String> {
    p.components()
        .filter(|c| !matches!(c, Component::CurDir | Component::Prefix(_)))
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}

fn is_under(p: &Path, base: &Path) -> bool {
    let (a, b) = (comps_lc(p), comps_lc(base));
    a.len() >= b.len() && a[..b.len()] == b[..]
}

fn depth_below(p: &Path, base: &Path) -> Option<usize> {
    is_under(p, base).then(|| comps_lc(p).len() - comps_lc(base).len())
}

/// Shared folders that may not be deleted themselves, but whose vendor sub-folders can be
/// (`AppData\Local\Microsoft\Edge`).
const DENY_FOLDER_ITSELF: &[&str] = &["microsoft", "packages", "temp", "windows", "microsoft shared"];

/// Folders nothing inside may be deleted from by a name match: shared runtimes and Windows parts.
/// (A program's own install folder inside one of these is still allowed: see `leftover_path_allowed`.)
const DENY_TREE: &[&str] = &[
    "common files", "windowsapps", "windows defender", "windows defender advanced threat protection",
    "windows nt", "windows mail", "windows media player", "windows photo viewer", "windows security",
    "windowspowershell", "internet explorer", "reference assemblies", "microsoft.net", "msbuild",
    "modifiablewindowsapps", "uninstall information", "installshield installation information",
    "package cache", "system32", "syswow64", "winsxs", "dotnet", "microsoft visual studio", "windows kits",
];

/// May this path be deleted as a program leftover?
/// Allowed: a folder/file at least one level below Program Files, ProgramData or an AppData root,
/// or anything inside the program's own install folder (when that folder itself is safe).
/// Refused: roots themselves, the Windows folder, shared folders (`FOLDER_DENY`), user documents.
pub fn leftover_path_allowed(path: &Path, install_location: Option<&Path>, roots: &Roots) -> bool {
    let comps = comps_lc(path);
    // Never a drive/profile root, never the Windows folder.
    if comps.len() < 3 {
        return false;
    }
    if let Some(sr) = &roots.system_root {
        if is_under(path, sr) {
            return false;
        }
    }
    let hard_blocked = |p: &Path| -> bool {
        roots.container_roots().iter().any(|r| comps_lc(p) == comps_lc(r))
            || comps_lc(p).iter().any(|c| matches!(c.as_str(), "system32" | "syswow64" | "winsxs" | "windowsapps"))
    };
    // Inside the program's own folder: fine, as long as that folder is a real program folder
    // (not a root or a shared folder). Otherwise the generic rules below decide.
    if let Some(loc) = install_location {
        if is_under(path, loc) {
            let loc_ok = !hard_blocked(loc)
                && roots.container_roots().iter().any(|r| depth_below(loc, r).map(|d| d >= 1).unwrap_or(false));
            let loc_shared = comps_lc(loc).last().map(|l| matches!(l.as_str(), "common files" | "microsoft shared")).unwrap_or(true);
            if loc_ok && !loc_shared {
                return true;
            }
        }
    }
    // Start-menu entries (a program's shortcut or its own folder) and desktop shortcuts. Checked
    // before the generic roots because the Start menu sits under AppData\Roaming\Microsoft.
    let ext_lnk = path.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false);
    if roots.start_menu.iter().any(|r| depth_below(path, r).map(|d| d >= 1).unwrap_or(false))
        || (ext_lnk && roots.desktop.iter().any(|r| depth_below(path, r).map(|d| d >= 1).unwrap_or(false)))
    {
        return true;
    }
    for r in roots.container_roots() {
        if let Some(d) = depth_below(path, r) {
            if d == 0 {
                return false;
            }
            // The first folder below the root decides what is shared / system territory.
            let first = comps[comps_lc(r).len()].as_str();
            if DENY_TREE.contains(&first) || (d == 1 && DENY_FOLDER_ITSELF.contains(&first)) {
                return false;
            }
            return true;
        }
    }
    false
}

/// May this registry key (relative to its hive) be deleted as a program leftover?
/// Only vendor/program keys under `Software` (or a service key) — never the OS's own branches.
pub fn reg_key_allowed(hive: Hive, path: &str) -> bool {
    let p = path.replace('/', r"\");
    let parts: Vec<String> = p.split('\\').filter(|s| !s.is_empty()).map(|s| s.to_lowercase()).collect();
    if parts.is_empty() {
        return false;
    }
    let ci = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    // Service keys: HKLM\SYSTEM\CurrentControlSet\Services\<name>
    if hive == Hive::Machine && parts.len() == 4 && parts[0] == "system" && ci(&parts[1], "currentcontrolset") && ci(&parts[2], "services") {
        return true;
    }
    if parts[0] != "software" {
        return false;
    }
    // Skip the WOW6432Node view.
    let rest: &[String] = if parts.get(1).map(|s| s == "wow6432node").unwrap_or(false) { &parts[2..] } else { &parts[1..] };
    let Some(vendor) = rest.first() else { return false };
    // Uninstall entries of the program itself.
    if rest.len() >= 5 && vendor == "microsoft" && rest[1] == "windows" && rest[2] == "currentversion" && rest[3] == "uninstall" {
        return true;
    }
    const OS_BRANCHES: &[&str] = &[
        "classes", "policies", "clients", "registeredapplications", "odbc", "wow6432node", "windows",
        "javasoft", "setup", "partner", "ole", "rpc", "cryptography", "dolby", "intel", "tracing",
    ];
    if OS_BRANCHES.contains(&vendor.as_str()) {
        return false;
    }
    if vendor == "microsoft" {
        // Only a product two levels down, and never Windows/OS branches.
        const MS_DENY: &[&str] = &[
            "windows", "windows nt", "windows defender", "cryptography", "internet explorer", ".netframework",
            "shared tools", "active setup", "driverinstall", "directx", "wbem", "ole", "rpc", "security center",
            "systemcertificates", "ctf", "input", "multimedia", "office", "microsoft", "tpm",
        ];
        let Some(product) = rest.get(1) else { return false };
        if MS_DENY.contains(&product.as_str()) {
            return false;
        }
        return true;
    }
    true
}

// ---- uninstall commands ---------------------------------------------------------------------

/// `MsiExec.exe /I{GUID}` is what Windows stores for "modify"; removing needs `/X`.
pub fn normalize_uninstall(cmd: &str) -> String {
    let lower = cmd.to_lowercase();
    if lower.contains("msiexec") {
        if let Some(i) = lower.find("/i{") {
            let mut s = cmd.to_string();
            s.replace_range(i..i + 2, "/X");
            return s;
        }
    }
    cmd.trim().to_string()
}

/// Splits an uninstall command line into (program, arguments): `"C:\x y\u.exe" /S` or `C:\x\u.exe /S`.
pub fn split_command(cmd: &str) -> (String, String) {
    let c = cmd.trim();
    if let Some(rest) = c.strip_prefix('"') {
        return match rest.find('"') {
            Some(end) => (rest[..end].to_string(), rest[end + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
    }
    let lower = c.to_lowercase();
    for ext in [".exe", ".msi", ".bat", ".cmd"] {
        if let Some(i) = lower.find(ext) {
            let end = i + ext.len();
            return (c[..end].to_string(), c[end..].trim().to_string());
        }
    }
    match c.split_once(char::is_whitespace) {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (c.to_string(), String::new()),
    }
}

/// Chromium-style installers (Edge, WebView2, Chrome) refuse `--uninstall` for components the
/// vendor marks as non-removable unless `--force-uninstall` is added.
pub fn force_variant(cmd: &str) -> Option<String> {
    let l = cmd.to_lowercase();
    (l.contains("setup.exe") && l.contains("--uninstall") && !l.contains("--force-uninstall"))
        .then(|| format!("{} --force-uninstall", cmd.trim()))
}

/// `YYYYMMDD` as stored by installers → unix seconds (UTC midnight).
pub fn parse_install_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() != 8 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (s[..4].parse().ok()?, s[4..6].parse().ok()?, s[6..8].parse().ok()?);
    if !(1990..=2200).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // days from civil (Howard Hinnant)
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// Windows FILETIME → unix seconds; `None` for 0 / pre-1990 garbage.
pub fn filetime_to_unix(ft: u64) -> Option<i64> {
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    (secs > 631_152_000).then_some(secs)
}

pub fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

/// Executables that run by themselves (updaters, tray helpers, services, installers). Their
/// launches say nothing about the user opening the program, so they do not count as "used".
pub fn is_background_exe(file_name: &str) -> bool {
    const HINTS: &[&str] = &[
        "helper", "updat", "agent", "service", "crash", "notif", "sync", "daemon", "tray", "unins", "setup",
        "install", "elevat", "broker", "host", "reporter", "monitor", "watchdog", "launcher", "bootstrap",
        "autoupdate", "maintenance", "repair", "report", "webview",
    ];
    let n = file_name.to_lowercase();
    HINTS.iter().any(|h| n.contains(h))
}

/// Processes whose executable lives inside `dir` (the program is still running).
pub fn processes_under(dir: &Path) -> Vec<crate::core::memory::ProcGroup> {
    let (_, groups) = crate::core::memory::snapshot();
    groups
        .into_iter()
        .filter(|g| g.exe.as_deref().map(|e| is_under(e, dir)).unwrap_or(false))
        .collect()
}

pub fn human_days(days: i64) -> String {
    let (tr, en) = match days {
        0 => ("bugün".to_string(), "today".to_string()),
        1..=59 => (format!("{days} gün önce"), format!("{days} days ago")),
        60..=729 => (format!("{} ay önce", days / 30), format!("{} months ago", days / 30)),
        _ => (format!("{} yıl önce", days / 365), format!("{} years ago", days / 365)),
    };
    crate::tr!(tr, en)
}

// ---- platform facade -------------------------------------------------------------------------
// The UI and the CLI call only these; Windows and macOS provide the implementations.

/// Does this platform have program management at all?
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

pub fn inventory(include_store: bool) -> Vec<InstalledApp> {
    #[cfg(windows)]
    {
        win::inventory(include_store)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = include_store;
        mac::inventory()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = include_store;
        Vec::new()
    }
}

/// Computes sizes the installers did not report.
pub fn fill_missing_sizes(apps: &mut [InstalledApp], cancel: &std::sync::atomic::AtomicBool) {
    for a in apps.iter_mut().filter(|a| a.size_bytes == 0) {
        if let Some(loc) = a.install_location.as_deref().filter(|p| p.is_dir()) {
            a.size_bytes = crate::core::scanner::dir_stats(loc, cancel).size;
        }
    }
}

/// What still belongs to `app` (before: a preview; after the uninstaller: the remnants).
pub fn find_leftovers(app: &InstalledApp, others: &[InstalledApp]) -> Vec<Leftover> {
    #[cfg(windows)]
    {
        win::find_leftovers(app, others, &win::ScanEnv::system())
    }
    #[cfg(target_os = "macos")]
    {
        mac::find_leftovers(app, others)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (app, others);
        Vec::new()
    }
}

/// Runs the vendor's uninstaller and waits. `force` = Chromium-style `--force-uninstall`.
pub fn run_uninstaller(app: &InstalledApp, silent: bool, force: bool) -> Result<i32, String> {
    #[cfg(windows)]
    {
        win::run_uninstaller(app, silent, force)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (silent, force);
        mac::remove_bundle(app)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (app, silent, force);
        Err("not supported on this platform".into())
    }
}

/// Is the program still installed (registry entry / bundle)? Polled while an uninstaller works.
pub fn still_registered(app: &InstalledApp) -> bool {
    #[cfg(windows)]
    {
        win::still_registered(app)
    }
    #[cfg(not(windows))]
    {
        app.install_location.as_deref().map(|p| p.exists()).unwrap_or(false)
    }
}

/// Force-closes whatever still runs from the program's folder.
pub fn close_running(app: &InstalledApp) -> crate::core::memory::CloseReport {
    let Some(loc) = app.install_location.as_deref() else { return Default::default() };
    let pids: Vec<u32> = processes_under(loc).iter().flat_map(|g| g.pids.clone()).collect();
    crate::core::memory::close_pids(&pids, true)
}

pub fn remove_leftovers(app: &InstalledApp, items: &[Leftover], opts: &RemoveOptions) -> RemoveReport {
    #[cfg(windows)]
    {
        win::remove_leftovers(app, items, opts, &win::ScanEnv::system())
    }
    #[cfg(target_os = "macos")]
    {
        mac::remove_leftovers(app, items, opts)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (app, items);
        RemoveReport { backup_dir: opts.backup_dir.clone(), ..Default::default() }
    }
}

/// A restore point / snapshot before the removal, where the OS offers one.
pub fn create_restore_point(description: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::create_restore_point(description)
    }
    #[cfg(not(windows))]
    {
        let _ = description;
        Err(crate::tr!("Bu platformda yok".to_string(), "Not available on this platform".to_string()))
    }
}

/// Puts back the registry keys saved in a backup folder (Windows). Returns how many files.
pub fn restore_backup(dir: &Path) -> Result<usize, String> {
    #[cfg(windows)]
    {
        win::restore_registry_backup(dir)
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Err(crate::tr!("Kayıt defteri yedeği yalnızca Windows'ta var".to_string(), "Registry backups exist on Windows only".to_string()))
    }
}

/// Extra step after removing a specific program (Edge: stop Windows Update putting it back).
/// `None` = nothing to do.
pub fn after_removal(app: &InstalledApp) -> Option<Result<String, String>> {
    #[cfg(windows)]
    {
        (app.name == "Microsoft Edge").then(|| {
            win::block_edge_reinstall()
                .map(|_| crate::tr!("Edge'in Windows Update ile geri gelmesi engellendi".to_string(), "Edge is blocked from coming back through Windows Update".to_string()))
        })
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        None
    }
}

/// Folder for this removal's registry exports and log.
pub fn backup_dir_for(app: &InstalledApp) -> PathBuf {
    let clean: String = app.name.chars().filter(|c| c.is_alphanumeric()).collect();
    crate::config::data_dir()
        .join("uninstall-backups")
        .join(format!("{}-{}", crate::models::now_secs(), clean))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots {
            program_files: vec![PathBuf::from(r"C:\Program Files"), PathBuf::from(r"C:\Program Files (x86)")],
            program_data: Some(PathBuf::from(r"C:\ProgramData")),
            local_appdata: Some(PathBuf::from(r"C:\Users\u\AppData\Local")),
            roaming_appdata: Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            local_low: Some(PathBuf::from(r"C:\Users\u\AppData\LocalLow")),
            start_menu: vec![PathBuf::from(r"C:\Users\u\AppData\Roaming\Microsoft\Windows\Start Menu\Programs")],
            desktop: vec![PathBuf::from(r"C:\Users\u\Desktop")],
            system_root: Some(PathBuf::from(r"C:\Windows")),
        }
    }

    #[test]
    fn variants_strip_version_architecture_and_year() {
        let v = name_variants("Foo Studio 2024 (64-bit) 5.1.2");
        assert!(v.contains(&"foostudio".to_string()), "{v:?}");
        assert!(name_variants("Go").is_empty());
        assert!(name_variants("7-Zip 23.01 (x64)").contains(&"7zip".to_string()));
    }

    #[test]
    fn exact_name_is_high_publisher_is_medium_only_when_unshared() {
        let v = name_variants("Cool Player");
        assert_eq!(match_name("Cool Player", &v, "Cool Inc", false).unwrap().0, Confidence::High);
        assert_eq!(match_name("cool-player", &v, "Cool Inc", false).unwrap().0, Confidence::High);
        assert_eq!(match_name("Cool Inc", &v, "Cool Inc", false).unwrap().0, Confidence::Medium);
        assert!(match_name("Cool Inc", &v, "Cool Inc", true).is_none(), "publisher folder shared with another app");
        assert!(match_name("Microsoft", &v, "Microsoft", false).is_none(), "shared vendor is never proof");
        assert!(match_name("Other Thing", &v, "Cool Inc", false).is_none());
        assert_eq!(match_name("Cool Inc Cool Player", &v, "Cool Inc", false).unwrap().0, Confidence::High);
    }

    #[test]
    fn leftover_paths_follow_the_guard() {
        let r = roots();
        let ok = |p: &str, loc: Option<&str>| leftover_path_allowed(Path::new(p), loc.map(Path::new), &r);
        assert!(ok(r"C:\Program Files\CoolPlayer", None));
        assert!(ok(r"C:\Users\u\AppData\Roaming\CoolPlayer\settings.json", None));
        assert!(ok(r"C:\ProgramData\CoolPlayer", None));
        // roots and shared / system folders are off limits
        assert!(!ok(r"C:\Program Files", None));
        assert!(!ok(r"C:\Program Files\Common Files", None));
        assert!(!ok(r"C:\Program Files\WindowsApps\X", None));
        assert!(!ok(r"C:\Windows\System32\drivers", None));
        assert!(!ok(r"C:\ProgramData\Microsoft", None));
        // ...but a vendor folder inside a shared one is fine, and shared runtimes stay protected
        assert!(ok(r"C:\Users\u\AppData\Local\Microsoft\Edge\User Data", None));
        assert!(!ok(r"C:\Program Files\dotnet\shared", None));
        assert!(!ok(r"C:\Program Files\Windows Defender\x", None));
        assert!(!ok(r"C:\Users\u\Documents\CoolPlayer", None));
        assert!(!ok(r"C:\Users\u", None));
        assert!(!ok(r"C:\", None));
        assert!(!ok(r"C:\Users\u\AppData\Local", None));
    }

    #[test]
    fn own_install_folder_is_allowed_even_under_a_vendor_folder_but_not_a_shared_root() {
        let r = roots();
        let loc = Path::new(r"C:\Program Files (x86)\Microsoft\Edge");
        assert!(leftover_path_allowed(Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application"), Some(loc), &r));
        assert!(leftover_path_allowed(loc, Some(loc), &r));
        // an install location that is a root/shared folder grants nothing extra
        assert!(!leftover_path_allowed(Path::new(r"C:\Program Files"), Some(Path::new(r"C:\Program Files")), &r));
        assert!(!leftover_path_allowed(
            Path::new(r"C:\Program Files\Common Files\X"),
            Some(Path::new(r"C:\Program Files\Common Files")),
            &r
        ));
        assert!(!leftover_path_allowed(Path::new(r"C:\Windows\Foo"), Some(Path::new(r"C:\Windows\Foo")), &r));
    }

    #[test]
    fn shortcuts_in_start_menu_and_desktop_are_allowed() {
        let r = roots();
        assert!(leftover_path_allowed(
            Path::new(r"C:\Users\u\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Cool Player.lnk"),
            None,
            &r
        ));
        assert!(leftover_path_allowed(Path::new(r"C:\Users\u\Desktop\Cool Player.lnk"), None, &r));
        assert!(!leftover_path_allowed(Path::new(r"C:\Users\u\Desktop\notes.docx"), None, &r));
    }

    #[test]
    fn background_executables_do_not_count_as_use() {
        for bg in ["ituneshelper.exe", "AdobeUpdater.exe", "GoogleCrashHandler.exe", "unins000.exe", "OneDriveSetup.exe", "msedgewebview2.exe"] {
            assert!(is_background_exe(bg), "{bg}");
        }
        for fg in ["chrome.exe", "Code.exe", "iTunes.exe", "Acrobat.exe", "msedge.exe", "obsidian.exe"] {
            assert!(!is_background_exe(fg), "{fg}");
        }
    }

    #[test]
    fn msi_modify_becomes_remove() {
        assert_eq!(normalize_uninstall("MsiExec.exe /I{ABC-1}"), "MsiExec.exe /X{ABC-1}");
        assert_eq!(normalize_uninstall("msiexec /x{ABC-1} /qn"), "msiexec /x{ABC-1} /qn");
        assert_eq!(normalize_uninstall(r#" "C:\a\u.exe" /S "#), r#""C:\a\u.exe" /S"#);
    }

    #[test]
    fn command_lines_split_into_program_and_arguments() {
        assert_eq!(
            split_command(r#""C:\Program Files\X\unins000.exe" /SILENT /NORESTART"#),
            (r"C:\Program Files\X\unins000.exe".to_string(), "/SILENT /NORESTART".to_string())
        );
        assert_eq!(
            split_command(r"C:\Program Files\X\unins000.exe /S"),
            (r"C:\Program Files\X\unins000.exe".to_string(), "/S".to_string())
        );
        assert_eq!(split_command("MsiExec.exe /X{ABC}"), ("MsiExec.exe".to_string(), "/X{ABC}".to_string()));
        assert_eq!(split_command(r#""C:\a b\u.exe""#), (r"C:\a b\u.exe".to_string(), String::new()));
    }

    #[test]
    fn chromium_installers_get_the_force_flag_once() {
        let edge = r#""C:\Program Files (x86)\Microsoft\Edge\Application\155.0\Installer\setup.exe" --uninstall --msedge --channel=stable --system-level --verbose-logging"#;
        let f = force_variant(edge).unwrap();
        assert!(f.ends_with("--force-uninstall"));
        assert!(force_variant(&f).is_none());
        assert!(force_variant(r#""C:\a\unins000.exe" /S"#).is_none());
    }

    #[test]
    fn dates_and_filetimes_convert() {
        assert_eq!(parse_install_date("20260101"), Some(1_767_225_600));
        assert_eq!(parse_install_date("1970-01"), None);
        assert_eq!(parse_install_date("20261340"), None);
        assert_eq!(filetime_to_unix(133_801_632_000_000_000), Some(1_735_689_600)); // 2025-01-01 00:00 UTC
        assert_eq!(filetime_to_unix(0), None);
        assert_eq!(rot13("Pnyphyngbe.rkr"), "Calculator.exe");
    }

    #[test]
    fn registry_guard_allows_vendor_keys_and_blocks_os_branches() {
        use Hive::*;
        assert!(reg_key_allowed(User, r"Software\CoolInc\CoolPlayer"));
        assert!(reg_key_allowed(Machine, r"SOFTWARE\WOW6432Node\CoolInc"));
        assert!(reg_key_allowed(Machine, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\CoolPlayer_is1"));
        assert!(reg_key_allowed(Machine, r"SYSTEM\CurrentControlSet\Services\CoolSvc"));
        assert!(reg_key_allowed(Machine, r"SOFTWARE\Microsoft\Edge"));
        assert!(reg_key_allowed(Machine, r"SOFTWARE\Microsoft\SomeProduct"));
        for bad in [
            "", "Software", r"Software\Microsoft", r"Software\Microsoft\Windows", r"Software\Microsoft\Windows NT\CurrentVersion",
            r"Software\Classes\.txt", r"Software\Policies\Foo", r"SYSTEM\CurrentControlSet", r"SYSTEM\CurrentControlSet\Services",
            r"SYSTEM\CurrentControlSet\Control\Foo", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion", r"Software\Wow6432Node", r"HARDWARE\Foo",
        ] {
            assert!(!reg_key_allowed(Machine, bad), "{bad:?} must be refused");
        }
    }
}
