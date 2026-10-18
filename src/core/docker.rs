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
