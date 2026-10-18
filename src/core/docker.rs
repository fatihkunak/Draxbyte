//! Read-only Docker disk check: `docker system df`. Never prunes or touches containers.

use crate::i18n::t;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerRow {
    pub kind: String,
    pub total: usize,
    pub active: usize,
    pub size: String,
    pub reclaimable: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerStatus {
    /// `docker` is not on PATH.
    NotInstalled,
    /// CLI found but the daemon did not answer (Docker Desktop stopped?).
    DaemonDown(String),
    Ok(Vec<DockerRow>),
}

/// `docker` command with the platform quirks applied.
/// A GUI app started from Finder gets PATH=/usr/bin:/bin:/usr/sbin:/sbin, which has no Docker.
fn docker_command() -> Command {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut c = Command::new("docker");
    #[cfg(target_os = "macos")]
    {
        let extra = "/usr/local/bin:/opt/homebrew/bin:/Applications/Docker.app/Contents/Resources/bin";
        let path = std::env::var("PATH").unwrap_or_default();
        c.env("PATH", format!("{extra}:{path}"));
    }
    c
}

fn command() -> Command {
    let mut c = docker_command();
    c.args(["system", "df", "--format", "{{json .}}"])
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from the GUI
    }
    c
}

/// Parse the one-JSON-object-per-line output of `docker system df --format '{{json .}}'`.
pub fn parse(stdout: &str) -> Vec<DockerRow> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .map(|v| {
            let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let n = |k: &str| s(k).parse().unwrap_or(0);
            DockerRow {
                kind: s("Type"),
                total: n("TotalCount"),
                active: n("Active"),
                size: s("Size"),
                reclaimable: s("Reclaimable"),
            }
        })
        .collect()
}

/// Runs `docker system df` with a timeout so a hung daemon cannot freeze the caller.
pub fn check() -> DockerStatus {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(command().output());
    });
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(out)) if out.status.success() => {
            DockerStatus::Ok(parse(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            DockerStatus::DaemonDown(err)
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => DockerStatus::NotInstalled,
        Ok(Err(e)) => DockerStatus::DaemonDown(e.to_string()),
        Err(_) => DockerStatus::DaemonDown(
            t("zaman aşımı (15 sn)", "timed out (15 s)").to_string(),
        ),
    }
}

/// Removes ONLY the Docker build cache (`docker builder prune -f`). Containers, images and
/// volumes are untouched. Returns Docker's summary line (e.g. "Total reclaimed space: 2GB").
pub fn prune_build_cache() -> Result<String, String> {
    let mut c = docker_command();
    c.args(["builder", "prune", "-f"]).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(c.output());
    });
    match rx.recv_timeout(Duration::from_secs(300)) {
        Ok(Ok(out)) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            Ok(s.lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string())
        }
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(t("zaman aşımı (5 dk)", "timed out (5 min)").to_string()),
    }
}

// ---- detailed optimization (used by the Full Optimization wizard) ---------------------------------

/// One thing Docker can clean up. Each is chosen separately by the user; `args` is exactly the
/// command that runs, nothing else is touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    StoppedContainers,
    BuildCache,
    DanglingImages,
    UnusedImages,
    Networks,
    Volumes,
}

impl Part {
    /// Run order: containers first, so the images they held become unused.
    pub const ALL: [Part; 6] = [
        Part::StoppedContainers,
        Part::BuildCache,
        Part::DanglingImages,
        Part::UnusedImages,
        Part::Networks,
        Part::Volumes,
    ];

    pub fn args(self) -> &'static [&'static str] {
        match self {
            Part::StoppedContainers => &["container", "prune", "-f"],
            Part::BuildCache => &["builder", "prune", "-a", "-f"],
            Part::DanglingImages => &["image", "prune", "-f"],
            Part::UnusedImages => &["image", "prune", "-a", "-f"],
            Part::Networks => &["network", "prune", "-f"],
            Part::Volumes => &["volume", "prune", "-f"],
        }
    }

    /// Can the removal lose something that cannot be rebuilt or re-downloaded?
    pub fn risky(self) -> bool {
        matches!(self, Part::StoppedContainers | Part::Volumes)
    }
}

/// Which parts the user ticked. The risky ones start unticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts([bool; 6]);

impl Default for Parts {
    fn default() -> Self {
        let mut p = Parts([false; 6]);
        for part in [Part::BuildCache, Part::DanglingImages, Part::Networks] {
            p.set(part, true);
        }
        p
    }
}

impl Parts {
    fn idx(p: Part) -> usize {
        Part::ALL.iter().position(|x| *x == p).unwrap_or(0)
    }
    pub fn has(&self, p: Part) -> bool {
        self.0[Self::idx(p)]
    }
    pub fn set(&mut self, p: Part, on: bool) {
        self.0[Self::idx(p)] = on;
    }
    pub fn any(&self) -> bool {
        self.0.iter().any(|b| *b)
    }
    pub fn none() -> Self {
        Parts([false; 6])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerItem {
    pub name: String,
    pub info: String,
    pub size: String,
}

/// What Docker holds that a clean-up could remove (read-only listing).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DockerDetail {
    pub rows: Vec<DockerRow>,
    pub stopped: Vec<DockerItem>,
    pub dangling_images: Vec<DockerItem>,
    pub volumes: Vec<DockerItem>,
    /// Size of Docker Desktop's virtual disk file on this machine; it does not shrink by itself.
    pub disk_image: Option<u64>,
}

impl DockerDetail {
    pub fn row(&self, kind: &str) -> Option<&DockerRow> {
        self.rows.iter().find(|r| r.kind.to_lowercase().contains(kind))
    }
}

/// Docker prints sizes like `2.005GB`, `512MB`, `0B`, `1.2kB` (decimal units).
pub fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().unwrap_or(0.0);
    let mult = match unit.trim().to_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

/// Bytes named in a prune summary such as `Total reclaimed space: 2.005GB` (0 when absent).
pub fn reclaimed_bytes(summary: &str) -> u64 {
    summary
        .to_lowercase()
        .split("reclaimed space:")
        .nth(1)
        .map(|rest| parse_size(rest.split_whitespace().next().unwrap_or("")))
        .unwrap_or(0)
}

fn json_items(stdout: &str, name: &[&str], info: &[&str], size: &str) -> Vec<DockerItem> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .map(|v| {
            let get = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let join = |keys: &[&str], sep: &str| {
                keys.iter().map(|k| get(k)).filter(|x| !x.is_empty() && x != "<none>").collect::<Vec<_>>().join(sep)
            };
            let mut n = join(name, ":");
            if n.is_empty() {
                n = get("ID");
            }
            DockerItem { name: n, info: join(info, " · "), size: get(size) }
        })
        .collect()
}

pub fn parse_stopped(stdout: &str) -> Vec<DockerItem> {
    json_items(stdout, &["Names"], &["Image", "Status"], "Size")
}

pub fn parse_images(stdout: &str) -> Vec<DockerItem> {
    json_items(stdout, &["Repository", "Tag"], &["CreatedSince"], "Size")
}

/// Docker names the volumes it creates by itself (no name given) with 64 hex characters; only
/// those are removed by `volume prune` on Docker 23 and newer.
pub fn is_anonymous_volume(name: &str) -> bool {
    name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit())
}

/// Unused volumes with their size, from `docker system df -v --format {{json .}}` (one JSON
/// object with a `Volumes` array), biggest first. Unused = no container links to it.
pub fn parse_df_volumes(stdout: &str) -> Vec<DockerItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout.trim()) else { return Vec::new() };
    let mut items: Vec<DockerItem> = v["Volumes"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|x| x["Links"].as_str().map(|l| l.trim() == "0").unwrap_or_else(|| x["Links"].as_i64() == Some(0)))
        .map(|x| {
            let name = x["Name"].as_str().unwrap_or_default().to_string();
            let kind = if is_anonymous_volume(&name) {
                t("anonim (otomatik oluşmuş)", "anonymous (auto-created)")
            } else {
                t("ADLI, elle oluşturulmuş: silinmez/dikkat", "NAMED, created on purpose: be careful")
            };
            let name = if is_anonymous_volume(&name) { format!("{}…", &name[..12]) } else { name };
            DockerItem { name, info: kind.to_string(), size: x["Size"].as_str().unwrap_or_default().to_string() }
        })
        .collect();
    items.sort_by(|a, b| parse_size(&b.size).cmp(&parse_size(&a.size)));
    items
}

pub fn parse_volumes(stdout: &str) -> Vec<DockerItem> {
    json_items(stdout, &["Name"], &["Driver"], "Size")
}

/// Runs a docker command, capturing stdout; `secs` bounds a hung daemon.
fn run_capture(args: &[&str], secs: u64) -> Result<String, String> {
    let mut c = docker_command();
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(c.output());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(out)) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(t("zaman aşımı", "timed out").to_string()),
    }
}

/// Size of Docker Desktop's virtual disk file, if this machine has one.
pub fn disk_image_size() -> Option<u64> {
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    #[cfg(windows)]
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        let base = std::path::PathBuf::from(l).join("Docker").join("wsl");
        cands.push(base.join("disk").join("docker_data.vhdx"));
        cands.push(base.join("data").join("ext4.vhdx"));
    }
    #[cfg(target_os = "macos")]
    {
        let base = crate::config::home_dir().join("Library/Containers/com.docker.docker/Data/vms/0/data");
        cands.push(base.join("Docker.raw"));
        cands.push(base.join("Docker.qcow2"));
    }
    cands.iter().find_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
}

/// Unused volumes with sizes (`system df -v` measures them, which can take a while); falls back
/// to the plain name list when that fails.
fn volumes_with_sizes() -> Vec<DockerItem> {
    if let Ok(out) = run_capture(&["system", "df", "-v", "--format", "{{json .}}"], 90) {
        let v = parse_df_volumes(&out);
        if !v.is_empty() || out.contains("\"Volumes\"") {
            return v;
        }
    }
    parse_volumes(&run_capture(&["volume", "ls", "--filter", "dangling=true", "--format", "{{json .}}"], 20).unwrap_or_default())
}

/// Read-only listing of what each part would remove. `rows` is the `check()` result.
pub fn detail(rows: Vec<DockerRow>) -> DockerDetail {
    let list = |args: &[&str]| run_capture(args, 20).unwrap_or_default();
    DockerDetail {
        rows,
        stopped: parse_stopped(&list(&["ps", "-a", "-s", "--filter", "status=exited", "--filter", "status=created", "--filter", "status=dead", "--format", "{{json .}}"])),
        dangling_images: parse_images(&list(&["image", "ls", "--filter", "dangling=true", "--format", "{{json .}}"])),
        volumes: volumes_with_sizes(),
        disk_image: disk_image_size(),
    }
}

/// Runs one clean-up part. Returns Docker's summary line (empty when it prints none).
pub fn prune(part: Part) -> Result<String, String> {
    let out = run_capture(part.args(), 300)?;
    Ok(out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string())
}

// ---- shrinking Docker Desktop's virtual disk (Windows, WSL2 backend) ----------------------------

/// The WSL2 virtual disk that holds Docker's data.
#[cfg(windows)]
pub fn disk_image_path() -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Docker").join("wsl");
    [base.join("disk").join("docker_data.vhdx"), base.join("data").join("ext4.vhdx")].into_iter().find(|p| p.is_file())
}

#[cfg(not(windows))]
pub fn disk_image_path() -> Option<std::path::PathBuf> {
    None
}

/// Why the disk image cannot be shrunk here (`None` = it can, as far as one can tell without trying).
pub fn compact_blocker() -> Option<String> {
    if !cfg!(windows) {
        return Some(t("Yalnızca Windows'ta (macOS'ta Docker boşalan alanı kendisi geri verir)", "Windows only (on macOS Docker gives freed space back by itself)").to_string());
    }
    if !crate::core::elevate::is_elevated() {
        return Some(t("Yönetici izni gerekir: Draxbyte'ı yönetici olarak yeniden aç", "Needs administrator rights: reopen Draxbyte as administrator").to_string());
    }
    if disk_image_path().is_none() {
        return Some(t("WSL2 disk imajı bulunamadı", "WSL2 disk image not found").to_string());
    }
    None
}

/// `wsl` prints UTF-16 (with NULs and sometimes a BOM); other tools print UTF-8.
pub fn decode_cli_text(raw: &[u8]) -> String {
    if raw.contains(&0) {
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    }
}

/// One name per non-empty line.
pub fn parse_names(raw: &[u8]) -> Vec<String> {
    decode_cli_text(raw)
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Running WSL distributions that are not Docker's own: stopping WSL stops these too.
pub fn other_wsl_running(running: &[String]) -> Vec<String> {
    running.iter().filter(|n| !n.to_lowercase().starts_with("docker-desktop")).cloned().collect()
}

/// The two diskpart scripts: compact, and detach (always run, even when compacting failed).
pub fn diskpart_scripts(vhdx: &std::path::Path) -> (String, String) {
    let sel = format!("select vdisk file=\"{}\"", vhdx.display());
    (format!("{sel}\nattach vdisk readonly\ncompact vdisk\ndetach vdisk\nexit\n"), format!("{sel}\ndetach vdisk\nexit\n"))
}

/// Containers that were running before and are not now.
pub fn containers_to_restore(before: &[String], now: &[String]) -> Vec<String> {
    before.iter().filter(|b| !now.contains(b)).cloned().collect()
}

/// Starts every container of `was_running` that is down, in that order, and checks again: up to
/// `rounds` passes, because a container can fail while the daemon is still settling. Returns
/// (started by us, still down at the end). `ps` lists the running names, `start` starts one.
pub fn restore_containers(
    was_running: &[String],
    rounds: u32,
    mut ps: impl FnMut() -> Vec<String>,
    mut start: impl FnMut(&str) -> bool,
    mut pause: impl FnMut(),
) -> (Vec<String>, Vec<String>) {
    let mut started: Vec<String> = Vec::new();
    for _ in 0..rounds {
        let todo = containers_to_restore(was_running, &ps());
        if todo.is_empty() {
            break;
        }
        for name in &todo {
            if start(name) && !started.contains(name) {
                started.push(name.clone());
            }
        }
        pause();
    }
    let down = containers_to_restore(was_running, &ps());
    started.retain(|n| !down.contains(n));
    (started, down)
}

/// Names of the running containers, oldest first.
pub fn running_container_names() -> Vec<String> {
    let mut v: Vec<String> = run_capture(&["ps", "--format", "{{.Names}}"], 20).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    v.reverse();
    v
}

/// Runs a program, bounded by `secs`; stdout on success, the output text on failure.
fn run_prog(program: &str, args: &[&str], secs: u64) -> Result<Vec<u8>, String> {
    let mut c = Command::new(program);
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(c.output());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(out)) if out.status.success() => Ok(out.stdout),
        Ok(Ok(out)) => Err(format!("{} {}", decode_cli_text(&out.stderr).trim(), decode_cli_text(&out.stdout).trim()).trim().to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(t("zaman aşımı", "timed out").to_string()),
    }
}

/// Running distributions (installed names ∩ the `--running` list, so a localized "none running"
/// message is never mistaken for a name).
pub fn running_wsl_distros() -> Vec<String> {
    let installed = run_prog("wsl", &["-l", "-q"], 15).map(|o| parse_names(&o)).unwrap_or_default();
    let running = run_prog("wsl", &["-l", "--running", "-q"], 15).map(|o| parse_names(&o)).unwrap_or_default();
    running.into_iter().filter(|r| installed.contains(r)).collect()
}

#[derive(Debug, Clone, Default)]
pub struct CompactReport {
    pub before: u64,
    pub after: u64,
    /// Containers that were running before and were started again.
    pub restored: Vec<String>,
    pub not_restored: Vec<String>,
    /// Something went wrong after Docker was stopped (Docker was still started again).
    pub error: Option<String>,
}

fn docker_down() -> bool {
    run_capture(&["info"], 10).is_err()
}

fn wait_until(mut done: impl FnMut() -> bool, tries: u32, every_secs: u64) -> bool {
    for _ in 0..tries {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(every_secs));
    }
    done()
}

fn start_docker_desktop() {
    let exe = std::env::var_os("ProgramFiles").map(|p| std::path::PathBuf::from(p).join("Docker").join("Docker").join("Docker Desktop.exe"));
    match exe.filter(|p| p.is_file()) {
        Some(p) => {
            let _ = Command::new(p).stdin(Stdio::null()).spawn();
        }
        None => {
            let _ = run_capture(&["desktop", "start"], 60);
        }
    }
}

/// Gives the space Docker freed inside its disk back to Windows. Stops Docker and every container,
/// shuts WSL down, compacts the virtual disk with diskpart, starts Docker again and starts the
/// containers that were running before. Nothing inside the disk is deleted.
///
/// `Err` = nothing was touched (blocked or Docker would not stop). `Ok` with `error` = Docker was
/// restarted and the containers restored as far as possible. `step` receives progress text.
pub fn compact_disk_image(mut step: impl FnMut(&str)) -> Result<CompactReport, String> {
    if let Some(b) = compact_blocker() {
        return Err(b);
    }
    let vhdx = disk_image_path().ok_or_else(|| t("WSL2 disk imajı bulunamadı", "WSL2 disk image not found").to_string())?;
    let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut rep = CompactReport { before: size(&vhdx), ..Default::default() };
    // `docker ps` lists the newest first; the oldest (usually databases) must come back first.
    let mut was_running: Vec<String> = run_capture(&["ps", "--format", "{{.Names}}"], 20)?.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    was_running.reverse();

    step(t("Docker kapatılıyor…", "Stopping Docker…"));
    run_capture(&["desktop", "stop"], 240).map_err(|e| format!("docker desktop stop: {e} ({})", t("Docker Desktop 4.37 veya yenisi gerekir", "needs Docker Desktop 4.37 or newer")))?;
    if !wait_until(docker_down, 40, 3) {
        return Err(t("Docker kapanmadı; hiçbir şey değiştirilmedi", "Docker did not stop; nothing was changed").to_string());
    }

    step(t("WSL kapatılıyor…", "Shutting WSL down…"));
    match run_prog("wsl", &["--shutdown"], 60) {
        Err(e) => rep.error = Some(format!("wsl --shutdown: {e}")),
        Ok(_) => {
            std::thread::sleep(Duration::from_secs(8));
            step(t("Disk imajı sıkıştırılıyor (birkaç dakika sürebilir)…", "Compacting the disk image (can take a few minutes)…"));
            let (compact, detach) = diskpart_scripts(&vhdx);
            let dir = std::env::temp_dir();
            let (f1, f2) = (dir.join("draxbyte-compact.txt"), dir.join("draxbyte-detach.txt"));
            if std::fs::write(&f1, compact).and_then(|_| std::fs::write(&f2, detach)).is_ok() {
                if let Err(e) = run_prog("diskpart", &["/s", &f1.to_string_lossy()], 1800) {
                    rep.error = Some(format!("diskpart: {e}"));
                }
                let _ = run_prog("diskpart", &["/s", &f2.to_string_lossy()], 120);
            } else {
                rep.error = Some(t("diskpart betiği yazılamadı", "could not write the diskpart script").to_string());
            }
            let _ = std::fs::remove_file(&f1);
            let _ = std::fs::remove_file(&f2);
        }
    }

    step(t("Docker yeniden açılıyor…", "Starting Docker again…"));
    start_docker_desktop();
    if !wait_until(|| !docker_down(), 60, 5) {
        rep.error.get_or_insert_with(|| t("Docker yeniden açılmadı; elle aç, konteynerler 'restart' politikasına bağlı", "Docker did not come back; open it by hand").to_string());
        rep.not_restored = was_running;
        rep.after = size(&vhdx);
        return Ok(rep);
    }
    std::thread::sleep(Duration::from_secs(15));
    step(t("Önceden çalışan konteynerler başlatılıyor…", "Starting the containers that were running…"));
    let (restored, down) = restore_containers(
        &was_running,
        4,
        || run_capture(&["ps", "--format", "{{.Names}}"], 20).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
        |n| run_capture(&["start", n], 60).is_ok(),
        || std::thread::sleep(Duration::from_secs(10)),
    );
    rep.restored = restored;
    rep.not_restored = down;
    rep.after = size(&vhdx);
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_system_df_lines() {
        let out = concat!(
            r#"{"Active":"17","Reclaimable":"0B (0%)","Size":"13.33GB","TotalCount":"17","Type":"Images"}"#,
            "\n",
            r#"{"Active":"0","Reclaimable":"2.005GB","Size":"2.719GB","TotalCount":"91","Type":"Build Cache"}"#,
            "\n"
        );
        let rows = parse(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "Images");
        assert_eq!(rows[0].total, 17);
        assert_eq!(rows[1].reclaimable, "2.005GB");
    }

    #[test]
    fn unused_volumes_come_with_sizes_biggest_first() {
        let anon = "a".repeat(64);
        let out = format!(
            r#"{{"Volumes":[{{"Name":"pg-data","Links":"1","Size":"34.3GB"}},{{"Name":"supabase_db","Links":"0","Size":"81.26MB"}},{{"Name":"{anon}","Links":"0","Size":"57.44MB"}},{{"Name":"{}","Links":"0","Size":"1.2GB"}}]}}"#,
            "b".repeat(64)
        );
        let v = parse_df_volumes(&out);
        assert_eq!(v.len(), 3, "the volume a container uses is not listed");
        assert_eq!(v[0].size, "1.2GB");
        assert_eq!(v[1].name, "supabase_db");
        assert_eq!(v[2].name, format!("{}…", "a".repeat(12)));
        assert!(v[1].info.contains("ADLI") || v[1].info.contains("NAMED"));
        assert!(is_anonymous_volume(&anon) && !is_anonymous_volume("supabase_db"));
        assert!(parse_df_volumes("garbage").is_empty());
    }

    #[test]
    fn wsl_output_is_decoded_and_only_real_names_survive() {
        let utf16: Vec<u8> = "\u{feff}Ubuntu\r\ndocker-desktop\r\n".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(parse_names(&utf16), vec!["Ubuntu", "docker-desktop"]);
        assert_eq!(parse_names(b"a\n\n b \n"), vec!["a", "b"]);
        let running = vec!["docker-desktop".to_string(), "Ubuntu".to_string()];
        assert_eq!(other_wsl_running(&running), vec!["Ubuntu"]);
        assert!(other_wsl_running(&["docker-desktop".to_string()]).is_empty());
    }

    #[test]
    fn diskpart_scripts_detach_even_when_compacting_fails() {
        let (c, d) = diskpart_scripts(std::path::Path::new(r"C:\x\docker_data.vhdx"));
        assert!(c.contains("attach vdisk readonly") && c.contains("compact vdisk") && c.contains("detach vdisk"));
        assert!(d.contains("detach vdisk") && !d.contains("compact"));
        assert!(c.contains("docker_data.vhdx\""));
    }

    #[test]
    fn only_containers_that_were_running_and_are_down_are_restored() {
        let before: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let now: Vec<String> = ["b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(containers_to_restore(&before, &now), vec!["a", "c"]);
        assert!(containers_to_restore(&[], &now).is_empty());
    }

    #[test]
    fn restore_retries_until_every_container_is_up_and_keeps_the_order() {
        use std::cell::RefCell;
        let was: Vec<String> = ["db", "api", "web"].iter().map(|s| s.to_string()).collect();
        let up: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let order: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let attempts: RefCell<u32> = RefCell::new(0);
        let (started, down) = restore_containers(
            &was,
            4,
            || up.borrow().clone(),
            |n| {
                order.borrow_mut().push(n.to_string());
                // "api" fails on its first try, as when the daemon is still settling
                if n == "api" && *attempts.borrow() == 0 {
                    *attempts.borrow_mut() += 1;
                    return false;
                }
                up.borrow_mut().push(n.to_string());
                true
            },
            || {},
        );
        assert_eq!(started, vec!["db", "web", "api"]);
        assert!(down.is_empty());
        assert_eq!(&order.borrow()[..3], &["db", "api", "web"], "first pass follows the original order");
    }

    #[test]
    fn restore_reports_what_could_not_be_started() {
        let was: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let up = std::cell::RefCell::new(Vec::<String>::new());
        let (started, down) = restore_containers(&was, 3, || up.borrow().clone(), |n| if n == "a" { up.borrow_mut().push("a".into()); true } else { false }, || {});
        assert_eq!(started, vec!["a"]);
        assert_eq!(down, vec!["b"]);
        // nothing to do when everything already runs
        let (s2, d2) = restore_containers(&was, 3, || was.clone(), |_| panic!("must not start anything"), || {});
        assert!(s2.is_empty() && d2.is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn compaction_is_honestly_unsupported_off_windows() {
        assert!(compact_blocker().is_some());
        assert!(compact_disk_image(|_| {}).is_err());
    }

    #[test]
    fn sizes_and_prune_summaries_are_read() {
        assert_eq!(parse_size("2.005GB"), 2_005_000_000);
        assert_eq!(parse_size("512MB"), 512_000_000);
        assert_eq!(parse_size("1.5kB"), 1500);
        assert_eq!(parse_size("0B"), 0);
        assert_eq!(parse_size("junk"), 0);
        assert_eq!(reclaimed_bytes("Total reclaimed space: 2.719GB"), 2_719_000_000);
        assert_eq!(reclaimed_bytes("Deleted Networks: x"), 0);
    }

    #[test]
    fn lists_are_parsed_and_unnamed_images_use_their_id() {
        let c = r#"{"Names":"db","Image":"postgres:16","Status":"Exited (0) 3 days ago","Size":"0B (virtual 400MB)"}"#;
        let st = parse_stopped(c);
        assert_eq!(st[0].name, "db");
        assert!(st[0].info.contains("postgres:16"));
        let i = r#"{"ID":"abc123","Repository":"<none>","Tag":"<none>","Size":"1.2GB","CreatedSince":"2 weeks ago"}"#;
        let im = parse_images(i);
        assert_eq!(im[0].name, "abc123");
        assert_eq!(im[0].size, "1.2GB");
        assert_eq!(parse_volumes(r#"{"Name":"vol1","Driver":"local"}"#)[0].name, "vol1");
        assert!(parse_stopped("garbage").is_empty());
    }

    #[test]
    fn risky_parts_start_unticked_and_commands_are_the_documented_ones() {
        let p = Parts::default();
        assert!(p.has(Part::BuildCache) && p.has(Part::DanglingImages) && p.has(Part::Networks));
        for part in Part::ALL {
            if part.risky() {
                assert!(!p.has(part), "{part:?} must be opt-in");
            }
            assert!(part.args().contains(&"prune"));
            assert!(part.args().contains(&"-f"));
        }
        // nothing here may ever be a whole-system prune
        assert!(Part::ALL.iter().all(|p| !p.args().contains(&"system")));
    }

    #[test]
    fn garbage_yields_no_rows() {
        assert!(parse("not json\n\n").is_empty());
    }
}
