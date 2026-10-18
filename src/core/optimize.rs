//! Full optimization: one plan made of the separate tools (cleanup, duplicates, Recycle Bin,
//! Docker build cache, memory). The user chooses the scope, `analyze` measures what each part
//! would do without touching anything, and only `apply` changes the machine.
//!
//! Nothing here is new, risky machinery: every deletion goes through `cleaner::clean`, so the
//! red category and `pre_delete_check` still apply, and everything goes to the Recycle Bin
//! except the explicit "empty the Recycle Bin" step.

use crate::config::Config;
use crate::core::cleaner::{self, CleanOptions};
use crate::core::docker::{self, DockerDetail, DockerStatus, Part, Parts};
use crate::core::dupes::{self, DupeFile, DupeGroup, DupeOptions, DupeProgress, Keep};
use crate::core::memory::{self, MemInfo};
use crate::core::recycle::{self, BinStatus};
use crate::core::scanner::{self, Progress, ScanOptions};
use crate::core::startup;
use crate::models::{Category, Outcome, ScanItem};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering::Relaxed};

/// Only the groups wasting the most are acted on, same limit as the Duplicates screen, so the
/// numbers the user approved are exactly what gets removed.
pub const DUPE_GROUP_CAP: usize = 300;

/// One part of the optimization, in the order `apply` runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Cleanup,
    Dupes,
    Docker,
    RecycleBin,
    Memory,
}

impl Task {
    pub const ALL: [Task; 5] = [Task::Cleanup, Task::Dupes, Task::Docker, Task::RecycleBin, Task::Memory];
}

/// What the user agreed to. `Default` is the full set; the UI/CLI switch parts off.
#[derive(Debug, Clone)]
pub struct Scope {
    pub cleanup: bool,
    pub dupes: bool,
    pub docker: bool,
    /// Which Docker clean-ups are ticked (used when `docker` is on).
    pub docker_parts: Parts,
    pub recycle_bin: bool,
    pub memory: bool,
    /// `None` = the configured default scan (all drives and system folders when enabled).
    pub scan_roots: Option<Vec<PathBuf>>,
    pub dupe_roots: Vec<PathBuf>,
    pub dupe_min_mb: u64,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            cleanup: true,
            dupes: true,
            docker: true,
            docker_parts: Parts::default(),
            recycle_bin: true,
            memory: true,
            scan_roots: None,
            dupe_roots: vec![crate::config::home_dir()],
            dupe_min_mb: 10,
        }
    }
}

impl Scope {
    pub fn has(&self, t: Task) -> bool {
        match t {
            Task::Cleanup => self.cleanup,
            Task::Dupes => self.dupes,
            Task::Docker => self.docker,
            Task::RecycleBin => self.recycle_bin,
            Task::Memory => self.memory,
        }
    }

    pub fn set(&mut self, t: Task, on: bool) {
        match t {
            Task::Cleanup => self.cleanup = on,
            Task::Dupes => self.dupes = on,
            Task::Docker => self.docker = on,
            Task::RecycleBin => self.recycle_bin = on,
            Task::Memory => self.memory = on,
        }
    }

    pub fn any(&self) -> bool {
        Task::ALL.iter().any(|t| self.has(*t))
    }
}

pub const STAGE_IDLE: u8 = 0;
pub const STAGE_SCAN: u8 = 1;
pub const STAGE_DUPES: u8 = 2;
pub const STAGE_PROBE: u8 = 3;
pub const STAGE_DONE: u8 = 4;

/// Live numbers for the progress screen (analysis and apply share one struct).
#[derive(Default)]
pub struct OptProgress {
    pub stage: AtomicU8,
    pub scan: Progress,
    pub dupes: DupeProgress,
    /// While applying: index into `Task::ALL` of the running part (`u8::MAX` = none yet).
    pub step: AtomicU8,
    pub clean_done: AtomicUsize,
    pub clean_total: AtomicUsize,
    pub freed: AtomicU64,
}

/// What `analyze` found. Read-only: building this never changes the machine.
#[derive(Default)]
pub struct Analysis {
    /// Safe (green) items a scan found.
    pub cleanup: Vec<ScanItem>,
    pub cleanup_bytes: u64,
    /// Paths the scan could not read.
    pub scan_errors: u64,
    /// Every group found, biggest waste first.
    pub dupe_groups: Vec<DupeGroup>,
    /// The copies that would be removed (oldest of each group stays), capped to `DUPE_GROUP_CAP`.
    pub dupe_delete: Vec<DupeFile>,
    pub dupe_bytes: u64,
    pub bin: Option<BinStatus>,
    /// What Docker holds (listing only); `None` when Docker is missing or not running.
    pub docker: Option<DockerDetail>,
    pub docker_state: Option<DockerStatus>,
    pub mem: Option<MemInfo>,
    /// Startup entries that are switched on. Shown as a hint only; never changed here.
    pub startup_enabled: usize,
    pub free_before: u64,
    pub cancelled: bool,
}

/// Green items only: never red, never yellow. (`pre_delete_check` re-checks every one at delete time.)
pub fn select_cleanup(items: &[ScanItem]) -> Vec<ScanItem> {
    items.iter().filter(|i| i.category == Category::Green).cloned().collect()
}

/// The copies to remove: from each of the first `DUPE_GROUP_CAP` groups everything except the
/// oldest file. A group never loses all its files.
pub fn select_dupes(groups: &[DupeGroup]) -> Vec<DupeFile> {
    groups
        .iter()
        .take(DUPE_GROUP_CAP)
        .flat_map(|g| {
            let keep = dupes::keep_index(g, Keep::Oldest);
            dupes::to_delete(g, keep).into_iter().cloned().collect::<Vec<_>>()
        })
        .collect()
}

/// Free bytes on the disk that holds the home folder (`0` when it cannot be told).
pub fn free_space() -> u64 {
    let home = crate::config::home_dir();
    sysinfo::Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|d| home.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .unwrap_or(0)
}

/// Can this Docker part remove anything right now? (Networks cannot be counted cheaply, so they always can.)
pub fn part_actionable(d: &DockerDetail, part: Part) -> bool {
    match part {
        Part::StoppedContainers => !d.stopped.is_empty(),
        Part::BuildCache => d.row("build").is_some_and(|r| r.total > 0),
        Part::DanglingImages => !d.dangling_images.is_empty(),
        Part::UnusedImages => d.row("image").is_some_and(|r| r.total > r.active),
        Part::Networks => true,
        Part::Volumes => !d.volumes.is_empty(),
    }
}

/// Space the chosen Docker parts should free, from Docker's own "reclaimable" figures.
/// (All unused images already include the untagged ones, so they are not counted twice.)
pub fn docker_estimate(d: &DockerDetail, parts: Parts) -> u64 {
    let first = |s: &str| docker::parse_size(s.split_whitespace().next().unwrap_or(""));
    let rec = |k: &str| d.row(k).map(|r| first(&r.reclaimable)).unwrap_or(0);
    let mut total = 0;
    if parts.has(Part::BuildCache) {
        total += rec("build");
    }
    if parts.has(Part::UnusedImages) {
        total += rec("image");
    } else if parts.has(Part::DanglingImages) {
        total += d.dangling_images.iter().map(|i| first(&i.size)).sum::<u64>();
    }
    if parts.has(Part::Volumes) {
        total += rec("volume");
    }
    if parts.has(Part::StoppedContainers) {
        total += d.stopped.iter().map(|i| first(&i.size)).sum::<u64>();
    }
    total
}

/// Measures every chosen part. Slow (full scan, duplicate search); honours `cancel`.
pub fn analyze(cfg: &Config, scope: &Scope, cancel: &AtomicBool, p: &OptProgress) -> Analysis {
    let mut a = Analysis { free_before: free_space(), ..Default::default() };

    if scope.cleanup {
        p.stage.store(STAGE_SCAN, Relaxed);
        let opts = ScanOptions::from_config(cfg, scope.scan_roots.clone());
        let out = scanner::scan(&opts, cancel, &p.scan);
        a.scan_errors = out.errors.total();
        a.cleanup = select_cleanup(&out.items);
        a.cleanup_bytes = a.cleanup.iter().map(|i| i.size).sum();
        if out.cancelled {
            a.cancelled = true;
            p.stage.store(STAGE_DONE, Relaxed);
            return a;
        }
    }
    if scope.dupes && !scope.dupe_roots.is_empty() {
        p.stage.store(STAGE_DUPES, Relaxed);
        let opts = DupeOptions { roots: scope.dupe_roots.clone(), min_size: scope.dupe_min_mb * 1024 * 1024, skip_dev: true };
        a.dupe_groups = dupes::find(&opts, cancel, &p.dupes);
        if cancel.load(Relaxed) {
            a.cancelled = true;
            p.stage.store(STAGE_DONE, Relaxed);
            return a;
        }
        a.dupe_delete = select_dupes(&a.dupe_groups);
        a.dupe_bytes = a.dupe_delete.iter().map(|f| f.size).sum();
    }
    p.stage.store(STAGE_PROBE, Relaxed);
    if scope.recycle_bin {
        a.bin = recycle::status().ok();
    }
    if scope.docker {
        let st = docker::check();
        if let DockerStatus::Ok(rows) = &st {
            a.docker = Some(docker::detail(rows.clone()));
        }
        a.docker_state = Some(st);
    }
    if scope.memory {
        a.mem = Some(memory::snapshot().0);
    }
    a.startup_enabled = startup::list().iter().filter(|e| e.enabled).count();
    p.stage.store(STAGE_DONE, Relaxed);
    a
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Done,
    /// Nothing to do, or not possible on this machine (with the reason).
    Skipped(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub task: Task,
    pub status: StepStatus,
    pub freed: u64,
    /// One line for the report (counts, Docker's summary, ...).
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub steps: Vec<StepResult>,
    pub free_before: u64,
    pub free_after: u64,
    /// The cleaner reports of the file steps, for the "undo" button.
    pub clean_reports: Vec<crate::models::CleanReport>,
}

impl RunReport {
    pub fn freed(&self) -> u64 {
        self.steps.iter().filter(|s| s.task != Task::Memory).map(|s| s.freed).sum()
    }
}

fn count_ok(rep: &crate::models::CleanReport) -> (usize, usize) {
    let ok = rep.entries.iter().filter(|e| matches!(e.outcome, Outcome::Trashed | Outcome::Deleted)).count();
    (ok, rep.entries.len() - ok)
}

/// Runs the chosen parts, in `Task::ALL` order (files first, then the Recycle Bin so what was just
/// moved there goes too, memory last). `permanent` skips the Recycle Bin for the file steps; the
/// interface never sets it.
pub fn apply(an: &Analysis, scope: &Scope, permanent: bool, cancel: &AtomicBool, p: &OptProgress) -> RunReport {
    let mut rep = RunReport { free_before: an.free_before, ..Default::default() };
    p.step.store(u8::MAX, Relaxed);
    for (idx, task) in Task::ALL.iter().copied().enumerate() {
        if !scope.has(task) {
            continue;
        }
        if cancel.load(Relaxed) {
            rep.steps.push(StepResult { task, status: StepStatus::Skipped(crate::i18n::t("iptal edildi", "cancelled").into()), freed: 0, detail: String::new() });
            continue;
        }
        p.step.store(idx as u8, Relaxed);
        let r = match task {
            Task::Cleanup => file_step(task, &an.cleanup, permanent, cancel, p, &mut rep),
            Task::Dupes => {
                let refs: Vec<&DupeFile> = an.dupe_delete.iter().collect();
                file_step(task, &dupes::to_scan_items(&refs), permanent, cancel, p, &mut rep)
            }
            Task::Docker => docker_step(an, scope.docker_parts),
            Task::RecycleBin => bin_step(),
            Task::Memory => memory_step(),
        };
        if task != Task::Memory {
            p.freed.fetch_add(r.freed, Relaxed);
        }
        rep.steps.push(r);
    }
    rep.free_after = free_space();
    p.stage.store(STAGE_DONE, Relaxed);
    rep
}

fn file_step(task: Task, items: &[ScanItem], permanent: bool, cancel: &AtomicBool, p: &OptProgress, rep: &mut RunReport) -> StepResult {
    if items.is_empty() {
        return StepResult { task, status: StepStatus::Skipped(crate::i18n::t("yapılacak bir şey yok", "nothing to do").into()), freed: 0, detail: String::new() };
    }
    p.clean_done.store(0, Relaxed);
    p.clean_total.store(items.len(), Relaxed);
    let cr = cleaner::clean(items, CleanOptions { dry_run: false, permanent }, cancel, |d, _| p.clean_done.store(d, Relaxed));
    let (ok, skipped) = count_ok(&cr);
    let freed = cr.freed_bytes;
    rep.clean_reports.push(cr);
    StepResult {
        task,
        status: StepStatus::Done,
        freed,
        detail: crate::tf!("{} öğe kaldırıldı, {} atlandı (kilitli/değişmiş/korumalı)", "{} items removed, {} skipped (locked/changed/protected)", ok, skipped),
    }
}

fn docker_step(an: &Analysis, parts: Parts) -> StepResult {
    let task = Task::Docker;
    let Some(d) = &an.docker else {
        return StepResult { task, status: StepStatus::Skipped(crate::i18n::t("Docker yok ya da çalışmıyor", "Docker is missing or not running").into()), freed: 0, detail: String::new() };
    };
    let (mut freed, mut ok, mut notes, mut first_err) = (0u64, 0usize, Vec::new(), None);
    for part in Part::ALL {
        if !parts.has(part) || !part_actionable(d, part) {
            continue;
        }
        match docker::prune(part) {
            Ok(summary) => {
                ok += 1;
                freed += docker::reclaimed_bytes(&summary);
                if !summary.is_empty() {
                    notes.push(summary);
                }
            }
            Err(e) => {
                notes.push(format!("{:?}: {e}", part));
                first_err.get_or_insert(e);
            }
        }
    }
    let detail = notes.join(" | ");
    match (ok, first_err) {
        (0, Some(e)) => StepResult { task, status: StepStatus::Failed(e), freed: 0, detail },
        (0, None) => StepResult { task, status: StepStatus::Skipped(crate::i18n::t("yapılacak bir şey yok", "nothing to do").into()), freed: 0, detail },
        _ => StepResult { task, status: StepStatus::Done, freed, detail },
    }
}

fn bin_step() -> StepResult {
    let task = Task::RecycleBin;
    let before = match recycle::status() {
        Ok(s) => s,
        Err(e) => return StepResult { task, status: StepStatus::Failed(e), freed: 0, detail: String::new() },
    };
    if before.items == 0 {
        return StepResult { task, status: StepStatus::Skipped(crate::i18n::t("Çöp Kutusu zaten boş", "the bin is already empty").into()), freed: 0, detail: String::new() };
    }
    match recycle::empty() {
        Ok(()) => StepResult { task, status: StepStatus::Done, freed: before.bytes, detail: crate::tf!("{} öğe kalıcı silindi", "{} items deleted for good", before.items) },
        Err(e) => StepResult { task, status: StepStatus::Failed(e), freed: 0, detail: String::new() },
    }
}

fn memory_step() -> StepResult {
    let task = Task::Memory;
    match memory::trim_working_sets() {
        Ok(r) => StepResult {
            task,
            status: StepStatus::Done,
            freed: r.available_after.saturating_sub(r.available_before),
            detail: crate::tf!("{} işlemin boşta belleği RAM'den çıkarıldı (geçici)", "idle memory of {} processes moved out of RAM (temporary)", r.trimmed),
        },
        Err(e) => StepResult { task, status: StepStatus::Skipped(e), freed: 0, detail: String::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn item(path: &str, cat: Category) -> ScanItem {
        ScanItem { path: path.into(), size: 10, mtime: 0, is_dir: false, category: cat, rule: String::new(), reason: String::new(), unknown: false }
    }

    #[test]
    fn cleanup_takes_green_only() {
        let all = vec![item("/a", Category::Green), item("/b", Category::Yellow), item("/c", Category::Red)];
        let sel = select_cleanup(&all);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].category, Category::Green);
    }

    fn group(n: usize, base: i64) -> DupeGroup {
        let files = (0..n).map(|i| DupeFile { path: format!("/x/{base}/{i}.bin").into(), size: 100, mtime: base + i as i64 }).collect();
        DupeGroup { size: 100, hash: format!("h{base}"), files }
    }

    #[test]
    fn dupes_keep_the_oldest_and_never_empty_a_group() {
        let groups = vec![group(3, 1000), group(2, 2000)];
        let del = select_dupes(&groups);
        assert_eq!(del.len(), 3);
        assert!(!del.iter().any(|f| f.mtime == 1000 || f.mtime == 2000), "the oldest of each group stays");
    }

    #[test]
    fn dupes_are_capped_to_what_the_user_saw() {
        let groups: Vec<DupeGroup> = (0..DUPE_GROUP_CAP as i64 + 25).map(|i| group(2, i * 10)).collect();
        assert_eq!(select_dupes(&groups).len(), DUPE_GROUP_CAP);
    }

    #[test]
    fn scope_toggles_by_task() {
        let mut s = Scope::default();
        assert!(Task::ALL.iter().all(|t| s.has(*t)));
        for t in Task::ALL {
            s.set(t, false);
        }
        assert!(!s.any());
        s.set(Task::Docker, true);
        assert!(s.any() && s.has(Task::Docker) && !s.has(Task::Cleanup));
    }

    #[test]
    fn docker_estimate_does_not_count_untagged_images_twice() {
        let row = |kind: &str, rec: &str| docker::DockerRow { kind: kind.into(), total: 5, active: 1, size: "9GB".into(), reclaimable: rec.into() };
        let item = |size: &str| docker::DockerItem { name: "x".into(), info: String::new(), size: size.into() };
        let d = DockerDetail {
            rows: vec![row("Images", "4GB (30%)"), row("Build Cache", "2GB"), row("Local Volumes", "1GB")],
            dangling_images: vec![item("1GB")],
            stopped: vec![item("0B (virtual 400MB)"), item("500MB (virtual 1GB)")],
            ..Default::default()
        };
        let mut p = Parts::none();
        p.set(Part::BuildCache, true);
        assert_eq!(docker_estimate(&d, p), 2_000_000_000);
        p.set(Part::DanglingImages, true);
        assert_eq!(docker_estimate(&d, p), 3_000_000_000);
        p.set(Part::UnusedImages, true);
        assert_eq!(docker_estimate(&d, p), 6_000_000_000, "unused images (4GB) replace the untagged 1GB");
        p.set(Part::StoppedContainers, true);
        p.set(Part::Volumes, true);
        assert_eq!(docker_estimate(&d, p), 7_500_000_000);
        assert_eq!(docker_estimate(&d, Parts::none()), 0);
    }

    #[test]
    fn docker_parts_are_actionable_only_when_there_is_something() {
        let row = |kind: &str, total, active| docker::DockerRow { kind: kind.into(), total, active, size: "1GB".into(), reclaimable: "0B".into() };
        let mut d = DockerDetail { rows: vec![row("Images", 3, 3), row("Build Cache", 0, 0)], ..Default::default() };
        assert!(!part_actionable(&d, Part::UnusedImages), "every image is in use");
        assert!(!part_actionable(&d, Part::BuildCache));
        assert!(!part_actionable(&d, Part::StoppedContainers));
        assert!(part_actionable(&d, Part::Networks));
        d.rows = vec![row("Images", 5, 3), row("Build Cache", 9, 0)];
        assert!(part_actionable(&d, Part::UnusedImages) && part_actionable(&d, Part::BuildCache));
    }

    /// End to end on a temp folder: analyze finds the copies, apply removes exactly the extras
    /// (permanently, so the test does not depend on a real Recycle Bin) and leaves one copy.
    #[test]
    fn duplicates_are_analyzed_and_removed_leaving_one_copy() {
        let dir = tempfile::tempdir().unwrap();
        let body = vec![7u8; 4096];
        for n in ["a.bin", "b.bin", "c.bin"] {
            fs::write(dir.path().join(n), &body).unwrap();
        }
        fs::write(dir.path().join("other.bin"), vec![9u8; 4096]).unwrap();
        let scope = Scope { cleanup: false, dupes: true, docker: false, recycle_bin: false, memory: false, dupe_roots: vec![dir.path().to_path_buf()], dupe_min_mb: 0, ..Scope::default() };
        let cfg = Config::default();
        let cancel = AtomicBool::new(false);
        let p = OptProgress::default();
        let an = analyze(&cfg, &scope, &cancel, &p);
        assert_eq!(an.dupe_groups.len(), 1);
        assert_eq!(an.dupe_delete.len(), 2);
        assert_eq!(an.dupe_bytes, 8192);
        // analysis changed nothing
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);

        let rep = apply(&an, &scope, true, &cancel, &p);
        assert_eq!(rep.steps.len(), 1);
        assert_eq!(rep.steps[0].status, StepStatus::Done);
        assert_eq!(rep.steps[0].freed, 8192);
        let left: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left.len(), 2, "one copy of the group plus the unrelated file: {left:?}");
        assert!(left.contains(&"other.bin".to_string()));
    }

    #[test]
    fn a_cancelled_run_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["a.bin", "b.bin"] {
            fs::write(dir.path().join(n), vec![1u8; 2048]).unwrap();
        }
        let scope = Scope { cleanup: false, dupes: true, docker: false, recycle_bin: false, memory: false, dupe_roots: vec![dir.path().to_path_buf()], dupe_min_mb: 0, ..Scope::default() };
        let p = OptProgress::default();
        let an = analyze(&Config::default(), &scope, &AtomicBool::new(false), &p);
        let rep = apply(&an, &scope, true, &AtomicBool::new(true), &p);
        assert!(matches!(rep.steps[0].status, StepStatus::Skipped(_)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
