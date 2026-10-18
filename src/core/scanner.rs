//! Parallel, cancellable, purely local scan. No network, no AI.

use crate::config::{expand_path, Config, RulesConfig};
use crate::core::rules::RuleEngine;
use crate::core::safety::{is_protected, is_root};
use crate::core::syslocs;
use crate::models::{now_secs, Category, ScanItem, ScanSummary};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub max_depth: usize,
    pub min_size: u64,
    pub exclude: Vec<String>,
    pub rules: RulesConfig,
}

impl ScanOptions {
    /// `roots: None` = the configured defaults plus, when enabled, the system folders and every
    /// internal drive. Explicit roots are scanned exactly as given.
    pub fn from_config(cfg: &Config, roots: Option<Vec<PathBuf>>) -> Self {
        Self {
            roots: roots.unwrap_or_else(|| default_roots(cfg)),
            max_depth: cfg.scan.max_depth,
            min_size: cfg.scan.min_size_mb * 1024 * 1024,
            exclude: cfg.scan.exclude_patterns.clone(),
            rules: cfg.rules.clone(),
        }
    }
}

/// The configured default paths, plus system folders / internal drives when switched on.
pub fn default_roots(cfg: &Config) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = cfg.scan.default_paths.iter().map(|p| expand_path(p)).collect();
    if cfg.scan.all_drives {
        v.extend(syslocs::fixed_drive_roots());
    }
    if cfg.scan.include_system {
        v.extend(syslocs::system_roots());
    }
    syslocs::dedupe_roots(v)
}

/// Why a path could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrKind {
    /// Access denied: needs administrator rights (Windows) or Full Disk Access (macOS).
    Permission,
    /// Open exclusively by another program (page file, a running database…).
    InUse,
    Other,
}

#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: PathBuf,
    pub kind: ErrKind,
    pub message: String,
}

/// `None` = not an error worth reporting: the file vanished between listing and reading
/// (temp files, caches), which says nothing about access.
pub fn classify_io_error(e: &std::io::Error) -> Option<ErrKind> {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => None,
        PermissionDenied => Some(ErrKind::Permission),
        // Raw codes are Windows' own (5 is EIO on Unix), so they are only read there.
        _ if cfg!(windows) => match e.raw_os_error() {
            Some(5) => Some(ErrKind::Permission),        // ERROR_ACCESS_DENIED
            Some(32) | Some(33) => Some(ErrKind::InUse), // sharing / lock violation
            Some(2) | Some(3) => None,                   // file / path not found
            _ => Some(ErrKind::Other),
        },
        _ => Some(ErrKind::Other),
    }
}

/// How many unreadable paths are kept as examples.
const MAX_ERROR_SAMPLES: usize = 400;

/// Live counters the UI polls while the scan runs.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub found: AtomicU64,
    /// Paths that could not be read (see `ScanErrors` for why). Vanished files are not counted.
    pub errors: AtomicU64,
    pub err_permission: AtomicU64,
    pub err_in_use: AtomicU64,
    pub error_samples: Mutex<Vec<ScanError>>,
    pub current: Mutex<String>,
}

impl Progress {
    /// Records an unreadable path (and keeps a few examples for the report).
    pub fn record_error(&self, path: &Path, e: &std::io::Error) {
        let Some(kind) = classify_io_error(e) else { return };
        self.errors.fetch_add(1, Relaxed);
        match kind {
            ErrKind::Permission => self.err_permission.fetch_add(1, Relaxed),
            ErrKind::InUse => self.err_in_use.fetch_add(1, Relaxed),
            ErrKind::Other => 0,
        };
        if let Ok(mut v) = self.error_samples.lock() {
            if v.len() < MAX_ERROR_SAMPLES {
                v.push(ScanError { path: path.to_path_buf(), kind, message: e.to_string() });
            }
        }
    }
}

/// What a scan could not read, so "inaccessible" is never a bare number.
#[derive(Debug, Clone, Default)]
pub struct ScanErrors {
    pub permission: u64,
    pub in_use: u64,
    pub other: u64,
    pub samples: Vec<ScanError>,
}

impl ScanErrors {
    pub fn total(&self) -> u64 {
        self.permission + self.in_use + self.other
    }
}

pub struct ScanOutput {
    pub items: Vec<ScanItem>,
    pub summary: ScanSummary,
    pub cancelled: bool,
    pub errors: ScanErrors,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DirStats {
    pub size: u64,
    /// Newest mtime among descendants (unix secs).
    pub newest: i64,
    pub files: u64,
}

pub fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Size and newest mtime of a folder. Symlinks are neither followed nor counted.
pub fn dir_stats(path: &Path, cancel: &AtomicBool) -> DirStats {
    dir_stats_report(path, cancel, &mut |_, _| {})
}

/// Like `dir_stats`, but tells `on_err` about every path that could not be read, so a folder
/// whose size is under-counted because part of it is unreadable is not silently wrong.
pub fn dir_stats_report(path: &Path, cancel: &AtomicBool, on_err: &mut dyn FnMut(&Path, &std::io::Error)) -> DirStats {
    let mut st = DirStats::default();
    for entry in WalkDir::new(path).follow_links(false) {
        if cancel.load(Relaxed) {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                if let (Some(p), Some(io)) = (e.path(), e.io_error()) {
                    on_err(p, io);
                }
                continue;
            }
        };
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        match entry.metadata() {
            Ok(meta) => {
                st.size += meta.len();
                st.files += 1;
                st.newest = st.newest.max(mtime_secs(&meta));
            }
            Err(e) => {
                if let Some(io) = e.io_error() {
                    on_err(entry.path(), io);
                }
            }
        }
    }
    st
}

fn build_excludes(patterns: &[String]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let p = p.replace('\\', "/");
        for g in [p.clone(), format!("**/{}", p)] {
            if let Ok(glob) = Glob::new(&g) {
                b.add(glob);
            }
        }
    }
    b.build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap())
}

struct Ctx<'a> {
    engine: RuleEngine,
    excludes: GlobSet,
    min_size: u64,
    cancel: &'a AtomicBool,
    progress: &'a Progress,
}

enum DirAction {
    Skip,
    Descend,
    Item(Box<ScanItem>),
}

impl Ctx<'_> {
    fn is_excluded(&self, path: &Path) -> bool {
        let s = path.to_string_lossy().replace('\\', "/");
        self.excludes.is_match(&s)
    }

    fn push(&self, out: &mut Vec<ScanItem>, item: ScanItem) {
        self.progress.found.fetch_add(1, Relaxed);
        out.push(item);
    }

    fn file_item(&self, path: &Path, size: u64, mtime: i64) -> Option<ScanItem> {
        if size < self.min_size {
            return None;
        }
        let v = self.engine.classify(path, size, mtime, false)?;
        Some(ScanItem {
            path: path.to_path_buf(),
            size,
            mtime,
            is_dir: false,
            category: v.category,
            rule: v.rule.to_string(),
            reason: v.reason,
            unknown: v.unknown,
        })
    }

    fn dir_action(&self, path: &Path) -> DirAction {
        if is_protected(path) || self.is_excluded(path) {
            return DirAction::Skip;
        }
        if !self.engine.wants_aggregate(path) {
            return DirAction::Descend;
        }
        let st = dir_stats_report(path, self.cancel, &mut |p, e| self.progress.record_error(p, e));
        self.progress.files.fetch_add(st.files, Relaxed);
        self.progress.bytes.fetch_add(st.size, Relaxed);
        if st.size < self.min_size {
            return DirAction::Skip;
        }
        match self.engine.classify(path, st.size, st.newest, true) {
            Some(v) => DirAction::Item(Box::new(ScanItem {
                path: path.to_path_buf(),
                size: st.size,
                mtime: st.newest,
                is_dir: true,
                category: v.category,
                rule: v.rule.to_string(),
                reason: v.reason,
                unknown: v.unknown,
            })),
            None => DirAction::Skip,
        }
    }

    fn visit_file(&self, entry: &walkdir::DirEntry, out: &mut Vec<ScanItem>) {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                if let Some(io) = e.io_error() {
                    self.progress.record_error(entry.path(), io);
                }
                return;
            }
        };
        let size = meta.len();
        let n = self.progress.files.fetch_add(1, Relaxed);
        self.progress.bytes.fetch_add(size, Relaxed);
        if n.is_multiple_of(512) {
            if let Ok(mut cur) = self.progress.current.try_lock() {
                *cur = entry.path().to_string_lossy().into_owned();
            }
        }
        if let Some(item) = self.file_item(entry.path(), size, mtime_secs(&meta)) {
            self.push(out, item);
        }
    }

    /// Sequential walk of one subtree; parallelism is across subtrees.
    fn walk(&self, root: &Path, max_depth: usize, out: &mut Vec<ScanItem>) {
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(max_depth)
            .into_iter();
        while let Some(next) = it.next() {
            if self.cancel.load(Relaxed) {
                return;
            }
            let entry = match next {
                Ok(e) => e,
                Err(e) => {
                    if let (Some(p), Some(io)) = (e.path(), e.io_error()) {
                        self.progress.record_error(p, io);
                    }
                    continue;
                }
            };
            let ft = entry.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if entry.depth() == 0 {
                    continue;
                }
                match self.dir_action(entry.path()) {
                    DirAction::Descend => {}
                    DirAction::Skip => it.skip_current_dir(),
                    DirAction::Item(item) => {
                        self.push(out, *item);
                        it.skip_current_dir();
                    }
                }
            } else if ft.is_file() {
                self.visit_file(&entry, out);
            }
        }
    }
}

pub fn scan(opts: &ScanOptions, cancel: &AtomicBool, progress: &Progress) -> ScanOutput {
    let started_at = now_secs();
    let ctx = Ctx {
        engine: RuleEngine::new(&opts.rules, started_at),
        excludes: build_excludes(&opts.exclude),
        min_size: opts.min_size,
        cancel,
        progress,
    };

    let mut items: Vec<ScanItem> = Vec::new();
    let mut subtrees: Vec<PathBuf> = Vec::new();

    for root in &opts.roots {
        // A drive root is "protected" (never deleted itself) but its children are scanned.
        if (is_protected(root) && !is_root(root)) || !root.is_dir() {
            continue;
        }
        let rd = match std::fs::read_dir(root) {
            Ok(rd) => rd,
            Err(e) => {
                progress.record_error(root, &e);
                continue;
            }
        };
        for entry in rd.filter_map(|e| e.ok()) {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ft.is_dir() {
                match ctx.dir_action(&path) {
                    DirAction::Descend => subtrees.push(path),
                    DirAction::Skip => {}
                    DirAction::Item(item) => ctx.push(&mut items, *item),
                }
            } else if ft.is_file() {
                if let Ok(meta) = entry.metadata() {
                    progress.files.fetch_add(1, Relaxed);
                    progress.bytes.fetch_add(meta.len(), Relaxed);
                    if let Some(item) = ctx.file_item(&path, meta.len(), mtime_secs(&meta)) {
                        ctx.push(&mut items, item);
                    }
                }
            }
        }
    }

    let depth = opts.max_depth.saturating_sub(1);
    let found: Vec<Vec<ScanItem>> = subtrees
        .par_iter()
        .map(|sub| {
            let mut out = Vec::new();
            ctx.walk(sub, depth, &mut out);
            out
        })
        .collect();
    items.extend(found.into_iter().flatten());

    // Overlapping roots (e.g. `~` and `~/AppData/Local/Temp`) must not double-count.
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(i.path.clone()));
    items.sort_by(|a, b| b.size.cmp(&a.size));

    let summary = ScanSummary {
        session_id: 0,
        started_at,
        finished_at: now_secs(),
        roots: opts
            .roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect(),
        item_count: items.len() as u64,
        total_bytes: items.iter().map(|i| i.size).sum(),
        green_bytes: items
            .iter()
            .filter(|i| i.category == Category::Green)
            .map(|i| i.size)
            .sum(),
        files_visited: progress.files.load(Relaxed),
        errors: progress.errors.load(Relaxed),
    };
    let samples = progress.error_samples.lock().map(|v| v.clone()).unwrap_or_default();
    let permission = progress.err_permission.load(Relaxed);
    let in_use = progress.err_in_use.load(Relaxed);
    let total = progress.errors.load(Relaxed);
    ScanOutput {
        items,
        summary,
        cancelled: cancel.load(Relaxed),
        errors: ScanErrors { permission, in_use, other: total.saturating_sub(permission + in_use), samples },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn set_old(path: &Path, days: u64) {
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    fn opts(root: &Path) -> ScanOptions {
        ScanOptions {
            roots: vec![root.to_path_buf()],
            max_depth: 10,
            min_size: 1024,
            exclude: vec![],
            rules: RulesConfig {
                large_file_mb: 1,
                ..RulesConfig::default()
            },
        }
    }

    fn run(o: &ScanOptions) -> ScanOutput {
        scan(o, &AtomicBool::new(false), &Progress::default())
    }

    #[test]
    fn finds_old_log_and_aggregates_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a").join("old.log");
        write(&log, 4096);
        set_old(&log, 90);
        write(&dir.path().join("proj/node_modules/x/a.js"), 3000);
        write(&dir.path().join("proj/node_modules/y/b.js"), 3000);
        write(&dir.path().join("proj/src/main.js"), 5000); // not interesting

        let out = run(&opts(dir.path()));
        let by_rule = |r: &str| out.items.iter().find(|i| i.rule == r);
        let l = by_rule("safe_ext").expect("old log found");
        assert_eq!(l.category, Category::Green);
        let nm = by_rule("node_modules").expect("node_modules aggregated");
        assert!(nm.is_dir);
        assert_eq!(nm.size, 6000);
        // contents of an aggregated dir are not listed separately
        assert!(!out.items.iter().any(|i| i.path.ends_with("a.js")));
        assert_eq!(out.items.len(), 2);
    }

    #[test]
    fn min_size_filters_small_items() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("tiny.log");
        write(&log, 100);
        set_old(&log, 90);
        assert!(run(&opts(dir.path())).items.is_empty());
    }

    #[test]
    fn exclude_patterns_prune_subtrees() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("skipme/old.log");
        write(&log, 4096);
        set_old(&log, 90);
        let mut o = opts(dir.path());
        o.exclude = vec!["skipme".into()];
        assert!(run(&o).items.is_empty());
        o.exclude = vec![];
        assert_eq!(run(&o).items.len(), 1);
    }

    #[test]
    fn max_depth_limits_the_walk() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a/b/c/old.log");
        write(&deep, 4096);
        set_old(&deep, 90);
        let mut o = opts(dir.path());
        o.max_depth = 2;
        assert!(run(&o).items.is_empty());
        o.max_depth = 10;
        assert_eq!(run(&o).items.len(), 1);
    }

    #[test]
    fn cancel_stops_early() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a/old.log");
        write(&log, 4096);
        set_old(&log, 90);
        let cancel = AtomicBool::new(true);
        let out = scan(&opts(dir.path()), &cancel, &Progress::default());
        assert!(out.cancelled);
        assert!(out.items.is_empty());
    }

    #[test]
    fn overlapping_roots_do_not_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("a/old.log");
        write(&log, 4096);
        set_old(&log, 90);
        let mut o = opts(dir.path());
        o.roots = vec![dir.path().to_path_buf(), dir.path().join("a")];
        assert_eq!(run(&o).items.len(), 1);
    }

    #[test]
    fn dir_stats_sums_and_tracks_newest() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("x/a"), 10);
        write(&dir.path().join("b"), 20);
        let st = dir_stats(dir.path(), &AtomicBool::new(false));
        assert_eq!((st.size, st.files), (30, 2));
        assert!(st.newest > 0);
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn vanished_files_are_not_errors_but_denied_and_in_use_are() {
        assert_eq!(classify_io_error(&Error::from(ErrorKind::NotFound)), None);
        assert_eq!(classify_io_error(&Error::from(ErrorKind::PermissionDenied)), Some(ErrKind::Permission));
        #[cfg(windows)]
        {
            assert_eq!(classify_io_error(&Error::from_raw_os_error(2)), None);
            assert_eq!(classify_io_error(&Error::from_raw_os_error(3)), None);
            assert_eq!(classify_io_error(&Error::from_raw_os_error(5)), Some(ErrKind::Permission));
            assert_eq!(classify_io_error(&Error::from_raw_os_error(32)), Some(ErrKind::InUse));
            assert_eq!(classify_io_error(&Error::from_raw_os_error(33)), Some(ErrKind::InUse));
            assert_eq!(classify_io_error(&Error::from_raw_os_error(1450)), Some(ErrKind::Other));
        }
        #[cfg(unix)]
        {
            // 13 = EACCES (PermissionDenied); 5 = EIO must NOT be mistaken for "access denied"
            assert_eq!(classify_io_error(&Error::from_raw_os_error(13)), Some(ErrKind::Permission));
            assert_eq!(classify_io_error(&Error::from_raw_os_error(5)), Some(ErrKind::Other));
        }
    }

    #[test]
    fn record_error_counts_by_kind_and_keeps_bounded_examples() {
        let p = Progress::default();
        p.record_error(Path::new("gone"), &Error::from(ErrorKind::NotFound));
        assert_eq!(p.errors.load(Relaxed), 0, "a vanished file is not counted");
        for i in 0..(MAX_ERROR_SAMPLES + 50) {
            p.record_error(Path::new(&format!("denied-{i}")), &Error::from(ErrorKind::PermissionDenied));
        }
        p.record_error(Path::new("weird"), &Error::new(ErrorKind::Other, "x"));
        assert_eq!(p.err_permission.load(Relaxed), (MAX_ERROR_SAMPLES + 50) as u64);
        assert_eq!(p.errors.load(Relaxed), (MAX_ERROR_SAMPLES + 51) as u64);
        assert_eq!(p.error_samples.lock().unwrap().len(), MAX_ERROR_SAMPLES);
    }

    /// A folder the current user may not list is reported (not silently under-counted).
    /// Skipped where no such folder exists or the test runs elevated.
    #[test]
    fn an_unreadable_folder_is_reported_by_dir_stats() {
        let candidates = if cfg!(windows) { vec![r"C:\System Volume Information"] } else { vec!["/private/var/db/sudo", "/root"] };
        let Some(dir) = candidates.into_iter().map(PathBuf::from).find(|p| std::fs::read_dir(p).is_err() && p.exists()) else {
            return;
        };
        let mut seen = Vec::new();
        dir_stats_report(&dir, &AtomicBool::new(false), &mut |p, e| seen.push((p.to_path_buf(), classify_io_error(e))));
        assert!(seen.iter().any(|(_, k)| *k == Some(ErrKind::Permission)), "{seen:?}");
    }
}
