//! Parallel, cancellable, purely local scan. No network, no AI.

use crate::config::{expand_path, Config, RulesConfig};
use crate::core::rules::RuleEngine;
use crate::core::safety::is_protected;
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
    pub fn from_config(cfg: &Config, roots: Option<Vec<PathBuf>>) -> Self {
        Self {
            roots: roots.unwrap_or_else(|| {
                cfg.scan
                    .default_paths
                    .iter()
                    .map(|p| expand_path(p))
                    .collect()
            }),
            max_depth: cfg.scan.max_depth,
            min_size: cfg.scan.min_size_mb * 1024 * 1024,
            exclude: cfg.scan.exclude_patterns.clone(),
            rules: cfg.rules.clone(),
        }
    }
}

/// Live counters the UI polls while the scan runs.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub found: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
}

pub struct ScanOutput {
    pub items: Vec<ScanItem>,
    pub summary: ScanSummary,
    pub cancelled: bool,
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
    let mut st = DirStats::default();
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if cancel.load(Relaxed) {
            break;
        }
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            st.size += meta.len();
            st.files += 1;
            st.newest = st.newest.max(mtime_secs(&meta));
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
        let st = dir_stats(path, self.cancel);
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
        let Ok(meta) = entry.metadata() else {
            self.progress.errors.fetch_add(1, Relaxed);
            return;
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
                Err(_) => {
                    self.progress.errors.fetch_add(1, Relaxed);
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
        if is_protected(root) || !root.is_dir() {
            continue;
        }
        let rd = match std::fs::read_dir(root) {
            Ok(rd) => rd,
            Err(_) => {
                progress.errors.fetch_add(1, Relaxed);
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
    ScanOutput {
        items,
        summary,
        cancelled: cancel.load(Relaxed),
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
