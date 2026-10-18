//! State and background jobs of the Disk map and Duplicates screens (the screens are in
//! `ui/diskmap_view.rs` and `ui/dupes_view.rs`; the work is in `core::diskmap` / `core::dupes`).

use crate::app::App;
use draxbyte::config::expand_path;
use draxbyte::core::cleaner::{self, CleanOptions};
use draxbyte::core::diskmap::{self, MapOptions, MapProgress, Node};
use draxbyte::core::dupes::{self, DupeGroup, DupeOptions, DupeProgress, Keep};
use draxbyte::i18n::t;
use draxbyte::models::CleanReport;
use draxbyte::tf;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Instant;

// ---- disk map ---------------------------------------------------------------------------------

pub struct MapJob {
    pub rx: Receiver<Node>,
    pub progress: Arc<MapProgress>,
    pub cancel: Arc<AtomicBool>,
    pub started: Instant,
}

pub struct DiskMapState {
    pub root_text: String,
    pub tree: Option<Node>,
    /// Child indices from the root to the folder being shown.
    pub path: Vec<usize>,
    pub job: Option<MapJob>,
    pub msg: Option<String>,
}

impl Default for DiskMapState {
    fn default() -> Self {
        // DRAXBYTE_MAP_ROOT=<path>: start with that folder in the box (screenshots, demos)
        let root_text = std::env::var("DRAXBYTE_MAP_ROOT").unwrap_or_else(|_| "~".into());
        Self { root_text, tree: None, path: Vec::new(), job: None, msg: None }
    }
}

impl DiskMapState {
    /// The folder currently shown.
    pub fn current(&self) -> Option<&Node> {
        let mut n = self.tree.as_ref()?;
        for &i in &self.path {
            n = n.children.get(i)?;
        }
        Some(n)
    }

    /// Entering `child` of the current folder (only real folders with content can be entered).
    pub fn enter(&mut self, child: usize) {
        if let Some(c) = self.current().and_then(|n| n.children.get(child)) {
            if c.is_dir && !c.other && !c.children.is_empty() {
                self.path.push(child);
            }
        }
    }
}

/// Only this many groups (the ones wasting the most) are shown, and deletion applies to exactly
/// those: nothing the user cannot see is ever deleted.
pub const MAX_GROUPS: usize = 300;

// ---- duplicates -------------------------------------------------------------------------------

pub struct DupeJob {
    pub rx: Receiver<Vec<DupeGroup>>,
    pub progress: Arc<DupeProgress>,
    pub cancel: Arc<AtomicBool>,
    pub started: Instant,
}

pub struct GroupUi {
    pub group: DupeGroup,
    /// `delete[i]` = the i-th file is ticked for deletion. At least one stays unticked.
    pub delete: Vec<bool>,
}

impl GroupUi {
    pub fn apply(&mut self, rule: Keep) {
        let keep = dupes::keep_index(&self.group, rule);
        self.delete = (0..self.group.files.len()).map(|i| i != keep).collect();
    }

    /// Ticks/unticks a file; refuses to tick the last copy that is still kept.
    pub fn set(&mut self, i: usize, on: bool) -> bool {
        if on && self.delete.iter().enumerate().filter(|(j, d)| *j != i && !**d).count() == 0 {
            return false;
        }
        self.delete[i] = on;
        true
    }

    pub fn selected(&self) -> Vec<&dupes::DupeFile> {
        self.group.files.iter().zip(&self.delete).filter(|(_, d)| **d).map(|(f, _)| f).collect()
    }
}

pub struct DupesState {
    pub roots_text: String,
    pub min_mb: u64,
    pub skip_dev: bool,
    pub job: Option<DupeJob>,
    pub groups: Vec<GroupUi>,
    pub searched: bool,
    pub rule: Keep,
    pub confirm: bool,
    pub permanent: bool,
    pub clean_job: Option<Receiver<CleanReport>>,
    pub report: Option<CleanReport>,
}

impl Default for DupesState {
    fn default() -> Self {
        Self {
            roots_text: std::env::var("DRAXBYTE_DUPES_ROOT").unwrap_or_else(|_| "~".into()),
            min_mb: 1,
            skip_dev: true,
            job: None,
            groups: Vec::new(),
            searched: false,
            rule: Keep::Oldest,
            confirm: false,
            permanent: false,
            clean_job: None,
            report: None,
        }
    }
}

impl DupesState {
    pub fn selected_count_bytes(&self) -> (usize, u64) {
        let mut n = 0;
        let mut b = 0;
        for g in self.groups.iter().take(MAX_GROUPS) {
            for f in g.selected() {
                n += 1;
                b += f.size;
            }
        }
        (n, b)
    }

    pub fn wasted(&self) -> u64 {
        self.groups.iter().map(|g| g.group.wasted()).sum()
    }
}

fn parse_paths(text: &str) -> Vec<PathBuf> {
    text.split(';').map(str::trim).filter(|s| !s.is_empty()).map(expand_path).collect()
}

impl App {
    // ---- disk map ----
    pub fn map_start(&mut self, ctx: &egui::Context) {
        if self.map.job.is_some() {
            return;
        }
        let root = parse_paths(&self.map.root_text).into_iter().next();
        let Some(root) = root.filter(|p| p.is_dir()) else {
            self.map.msg = Some(t("Klasör bulunamadı", "Folder not found").to_string());
            return;
        };
        self.map.msg = None;
        self.map.tree = None;
        self.map.path.clear();
        let (tx, rx) = channel();
        let progress = Arc::new(MapProgress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (p2, c2, ctx2) = (progress.clone(), cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            let tree = diskmap::build(&root, &MapOptions::default(), &c2, &p2);
            let _ = tx.send(tree);
            ctx2.request_repaint();
        });
        self.map.job = Some(MapJob { rx, progress, cancel, started: Instant::now() });
    }

    // ---- duplicates ----
    pub fn dupes_start(&mut self, ctx: &egui::Context) {
        if self.dupes.job.is_some() {
            return;
        }
        let roots = parse_paths(&self.dupes.roots_text);
        if roots.is_empty() {
            return;
        }
        self.dupes.groups.clear();
        self.dupes.report = None;
        self.dupes.confirm = false;
        let opts = DupeOptions { roots, min_size: self.dupes.min_mb.max(0) * 1024 * 1024, skip_dev: self.dupes.skip_dev };
        let (tx, rx) = channel();
        let progress = Arc::new(DupeProgress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (p2, c2, ctx2) = (progress.clone(), cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            let groups = dupes::find(&opts, &c2, &p2);
            let _ = tx.send(groups);
            ctx2.request_repaint();
        });
        self.dupes.job = Some(DupeJob { rx, progress, cancel, started: Instant::now() });
    }

    pub fn dupes_apply_rule(&mut self, rule: Keep) {
        self.dupes.rule = rule;
        for g in &mut self.dupes.groups {
            g.apply(rule);
        }
    }

    /// Moves/deletes the ticked copies through the cleaner (re-checked right before each deletion).
    pub fn dupes_delete(&mut self, ctx: &egui::Context) {
        let files: Vec<dupes::DupeFile> = self.dupes.groups.iter().take(MAX_GROUPS).flat_map(|g| g.selected().into_iter().cloned()).collect();
        if files.is_empty() || self.dupes.clean_job.is_some() {
            return;
        }
        self.dupes.confirm = false;
        let permanent = self.dupes.permanent;
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let refs: Vec<&dupes::DupeFile> = files.iter().collect();
            let items = dupes::to_scan_items(&refs);
            let rep = cleaner::clean(&items, CleanOptions { dry_run: false, permanent }, &AtomicBool::new(false), |_, _| {});
            let _ = tx.send(rep);
            ctx.request_repaint();
        });
        self.dupes.clean_job = Some(rx);
    }

    /// Called every frame from `panels_poll`.
    pub fn map_dupes_poll(&mut self) {
        if let Some(job) = &self.map.job {
            if let Ok(tree) = job.rx.try_recv() {
                let cancelled = job.cancel.load(std::sync::atomic::Ordering::Relaxed);
                self.map.job = None;
                if cancelled {
                    self.map.msg = Some(t("İptal edildi", "Cancelled").to_string());
                } else {
                    self.map.tree = Some(tree);
                }
            }
        }
        if let Some(job) = &self.dupes.job {
            if let Ok(groups) = job.rx.try_recv() {
                self.dupes.job = None;
                self.dupes.searched = true;
                let rule = self.dupes.rule;
                self.dupes.groups = groups
                    .into_iter()
                    .map(|g| {
                        let mut u = GroupUi { delete: vec![false; g.files.len()], group: g };
                        u.apply(rule);
                        u
                    })
                    .collect();
            }
        }
        if let Some(rx) = &self.dupes.clean_job {
            if let Ok(rep) = rx.try_recv() {
                self.dupes.clean_job = None;
                // forget what is gone
                let gone: std::collections::HashSet<&PathBuf> = rep
                    .entries
                    .iter()
                    .filter(|e| matches!(e.outcome, draxbyte::models::Outcome::Trashed | draxbyte::models::Outcome::Deleted))
                    .map(|e| &e.path)
                    .collect();
                for g in &mut self.dupes.groups {
                    let keep: Vec<bool> = g.group.files.iter().map(|f| !gone.contains(&f.path)).collect();
                    let mut i = 0;
                    g.group.files.retain(|_| {
                        i += 1;
                        keep[i - 1]
                    });
                    let mut j = 0;
                    g.delete.retain(|_| {
                        j += 1;
                        keep[j - 1]
                    });
                }
                self.dupes.groups.retain(|g| g.group.files.len() >= 2);
                let n = rep.entries.iter().filter(|e| matches!(e.outcome, draxbyte::models::Outcome::Trashed | draxbyte::models::Outcome::Deleted)).count();
                self.toast(tf!("{} dosya kaldırıldı · {} boşaldı", "{} files removed · {} freed", n, draxbyte::models::human_bytes(rep.freed_bytes)));
                self.dupes.report = Some(rep);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draxbyte::core::dupes::DupeFile;

    fn group(n: usize) -> GroupUi {
        let files = (0..n)
            .map(|i| DupeFile { path: PathBuf::from(format!("/x/{i}.bin")), size: 100, mtime: 100 + i as i64 })
            .collect();
        let g = DupeGroup { size: 100, hash: "h".into(), files };
        let mut u = GroupUi { delete: vec![false; n], group: g };
        u.apply(Keep::Oldest);
        u
    }

    #[test]
    fn auto_selection_keeps_exactly_one_copy() {
        let mut g = group(4);
        assert_eq!(g.delete, vec![false, true, true, true], "oldest kept");
        g.apply(Keep::Newest);
        assert_eq!(g.delete, vec![true, true, true, false]);
    }

    #[test]
    fn the_last_kept_copy_can_never_be_ticked() {
        let mut g = group(3);
        assert!(g.set(0, false));
        // currently file 0 kept, 1 and 2 ticked: ticking 0 must be refused
        assert!(!g.set(0, true));
        assert_eq!(g.delete, vec![false, true, true]);
        // unticking another file lets 0 be ticked afterwards
        assert!(g.set(1, false));
        assert!(g.set(0, true));
        assert_eq!(g.delete.iter().filter(|d| !**d).count(), 1);
        assert_eq!(g.selected().len(), 2);
    }

    #[test]
    fn map_navigation_only_enters_real_folders() {
        let leaf = Node { name: "f.bin".into(), path: "/r/f.bin".into(), size: 5, files: 1, is_dir: false, children: vec![], other: false };
        let inner = Node { name: "inner".into(), path: "/r/d/inner".into(), size: 4, files: 1, is_dir: true, children: vec![leaf.clone()], other: false };
        let empty_dir = Node { name: "empty".into(), path: "/r/e".into(), size: 1, files: 0, is_dir: true, children: vec![], other: false };
        let dir = Node { name: "d".into(), path: "/r/d".into(), size: 4, files: 1, is_dir: true, children: vec![inner], other: false };
        let other = Node { name: "3 small".into(), path: "/r".into(), size: 1, files: 3, is_dir: false, children: vec![], other: true };
        let root = Node { name: "r".into(), path: "/r".into(), size: 11, files: 5, is_dir: true, children: vec![dir, leaf, empty_dir, other], other: false };
        let mut s = DiskMapState { tree: Some(root), ..Default::default() };
        s.enter(1); // a file
        s.enter(2); // a folder with nothing kept
        s.enter(3); // the folded "other"
        assert!(s.path.is_empty());
        s.enter(0);
        assert_eq!(s.current().unwrap().name, "d");
        s.enter(0);
        assert_eq!(s.current().unwrap().name, "inner");
        s.path.clear();
        assert_eq!(s.current().unwrap().name, "r");
    }
}
