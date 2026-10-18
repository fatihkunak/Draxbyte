//! Duplicate finder: same size → same first/last 64 KiB → same SHA-256 of the whole file.
//! Nothing is read in full unless a cheaper test already says two files could be equal.
//!
//! Deleting goes through `cleaner` like any other cleanup (Recycle Bin by default, last-moment
//! re-check that the file did not change), and every group keeps at least one copy.

use crate::core::safety::is_protected;
use crate::models::{Category, ScanItem};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DupeOptions {
    pub roots: Vec<PathBuf>,
    /// Files smaller than this are ignored.
    pub min_size: u64,
    /// Skip `node_modules`, `.git`, build output and caches: their duplicates are by design.
    pub skip_dev: bool,
}

impl Default for DupeOptions {
    fn default() -> Self {
        Self { roots: Vec::new(), min_size: 1 << 20, skip_dev: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupeFile {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone)]
pub struct DupeGroup {
    pub size: u64,
    pub hash: String,
    /// Two or more identical files.
    pub files: Vec<DupeFile>,
}

impl DupeGroup {
    /// Bytes that deleting all but one copy would free.
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64 - 1)
    }
}

/// What the finder is doing right now (for the progress line).
pub const STAGE_INDEX: u8 = 0;
pub const STAGE_QUICK: u8 = 1;
pub const STAGE_FULL: u8 = 2;
pub const STAGE_DONE: u8 = 3;

#[derive(Default)]
pub struct DupeProgress {
    pub stage: AtomicU8,
    pub files_seen: AtomicU64,
    pub candidates: AtomicU64,
    pub hashed: AtomicU64,
    pub to_hash: AtomicU64,
}

const DEV_DIRS: &[&str] = &[
    "node_modules", ".git", ".svn", ".hg", "target", "__pycache__", ".gradle", ".cargo", ".npm", ".cache", "venv", ".venv",
    // build intermediates: identical copies are produced on purpose (Android/CMake, Xcode, web)
    "intermediates", ".cxx", "deriveddata", "pods", ".next", ".nuxt", ".expo", ".parcel-cache", "bower_components",
];

fn skip_dir(name: &str, skip_dev: bool) -> bool {
    skip_dev && DEV_DIRS.contains(&name.to_lowercase().as_str())
}

fn mtime_secs(m: &std::fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// SHA-256 of the first and last 64 KiB (the whole file when it is small) plus its length.
pub fn quick_hash(path: &Path, size: u64) -> std::io::Result<[u8; 32]> {
    const CHUNK: u64 = 64 * 1024;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    let mut buf = vec![0u8; CHUNK as usize];
    if size <= 2 * CHUNK {
        let mut all = Vec::with_capacity(size as usize);
        f.read_to_end(&mut all)?;
        h.update(&all);
    } else {
        f.read_exact(&mut buf)?;
        h.update(&buf);
        f.seek(SeekFrom::End(-(CHUNK as i64)))?;
        f.read_exact(&mut buf)?;
        h.update(&buf);
    }
    Ok(h.finalize().into())
}

pub fn full_hash(path: &Path, cancel: &AtomicBool) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Relaxed) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Finds identical files under `opts.roots`, biggest waste first.
pub fn find(opts: &DupeOptions, cancel: &AtomicBool, progress: &DupeProgress) -> Vec<DupeGroup> {
    progress.stage.store(STAGE_INDEX, Relaxed);
    // 1. index: group by size
    let mut by_size: HashMap<u64, Vec<DupeFile>> = HashMap::new();
    let mut seen_paths = std::collections::HashSet::new();
    for root in &opts.roots {
        let it = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                return !skip_dir(&name, opts.skip_dev) && !is_protected(e.path());
            }
            true
        });
        for e in it.flatten() {
            if cancel.load(Relaxed) {
                return Vec::new();
            }
            if !e.file_type().is_file() {
                continue;
            }
            progress.files_seen.fetch_add(1, Relaxed);
            let Ok(meta) = e.metadata() else { continue };
            let size = meta.len();
            if size < opts.min_size || !seen_paths.insert(e.path().to_path_buf()) {
                continue;
            }
            by_size.entry(size).or_default().push(DupeFile { path: e.path().to_path_buf(), size, mtime: mtime_secs(&meta) });
        }
    }
    let candidates: Vec<DupeFile> = by_size.into_values().filter(|v| v.len() >= 2).flatten().collect();
    progress.candidates.store(candidates.len() as u64, Relaxed);

    // 2. quick hash (first + last 64 KiB)
    progress.stage.store(STAGE_QUICK, Relaxed);
    let quick: Vec<(DupeFile, [u8; 32])> = candidates
        .into_par_iter()
        .filter_map(|f| {
            if cancel.load(Relaxed) {
                return None;
            }
            quick_hash(&f.path, f.size).ok().map(|h| (f, h))
        })
        .collect();
    let mut by_quick: HashMap<(u64, [u8; 32]), Vec<DupeFile>> = HashMap::new();
    for (f, h) in quick {
        by_quick.entry((f.size, h)).or_default().push(f);
    }
    let survivors: Vec<DupeFile> = by_quick.into_values().filter(|v| v.len() >= 2).flatten().collect();
    progress.to_hash.store(survivors.len() as u64, Relaxed);

    // 3. full hash
    progress.stage.store(STAGE_FULL, Relaxed);
    let full: Vec<(DupeFile, String)> = survivors
        .into_par_iter()
        .filter_map(|f| {
            let r = full_hash(&f.path, cancel).ok().map(|h| (f, h));
            progress.hashed.fetch_add(1, Relaxed);
            r
        })
        .collect();
    if cancel.load(Relaxed) {
        return Vec::new();
    }
    let mut by_full: HashMap<(u64, String), Vec<DupeFile>> = HashMap::new();
    for (f, h) in full {
        by_full.entry((f.size, h)).or_default().push(f);
    }
    let mut groups: Vec<DupeGroup> = by_full
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|((size, hash), mut files)| {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            DupeGroup { size, hash, files }
        })
        .collect();
    groups.sort_by(|a, b| b.wasted().cmp(&a.wasted()).then(a.hash.cmp(&b.hash)));
    progress.stage.store(STAGE_DONE, Relaxed);
    groups
}

/// Which copy to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Oldest,
    Newest,
    ShortestPath,
}

/// Index of the file to keep in `group` under `rule` (ties: the first by path).
pub fn keep_index(group: &DupeGroup, rule: Keep) -> usize {
    let key = |f: &DupeFile| -> (i64, usize, String) {
        match rule {
            Keep::Oldest => (f.mtime, f.path.as_os_str().len(), f.path.to_string_lossy().into_owned()),
            Keep::Newest => (-f.mtime, f.path.as_os_str().len(), f.path.to_string_lossy().into_owned()),
            Keep::ShortestPath => (f.path.as_os_str().len() as i64, 0, f.path.to_string_lossy().into_owned()),
        }
    };
    group.files.iter().enumerate().min_by_key(|(_, f)| key(f)).map(|(i, _)| i).unwrap_or(0)
}

/// The files to delete when `keep` survives in each group. Never includes the kept file.
pub fn to_delete(group: &DupeGroup, keep: usize) -> Vec<&DupeFile> {
    group.files.iter().enumerate().filter(|(i, _)| *i != keep).map(|(_, f)| f).collect()
}

/// Cleaner input: yellow items (the user chose them), re-checked right before deletion.
pub fn to_scan_items(files: &[&DupeFile]) -> Vec<ScanItem> {
    files
        .iter()
        .map(|f| ScanItem {
            path: f.path.clone(),
            size: f.size,
            mtime: f.mtime,
            is_dir: false,
            category: Category::Yellow,
            rule: "duplicate".into(),
            reason: "Duplicate of another file; one copy is kept".into(),
            unknown: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn opts(root: &Path) -> DupeOptions {
        DupeOptions { roots: vec![root.to_path_buf()], min_size: 100, skip_dev: true }
    }

    #[test]
    fn finds_identical_files_and_ignores_same_size_but_different_content() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let a = vec![7u8; 5000];
        let mut b = a.clone();
        b[2500] = 8; // same size and same first/last 64 KiB window? (small file → whole file hashed)
        write(&t.join("one/a.bin"), &a);
        write(&t.join("two/a_copy.bin"), &a);
        write(&t.join("three/a_again.bin"), &a);
        write(&t.join("four/different.bin"), &b);
        write(&t.join("tiny.bin"), &[1u8; 10]);
        write(&t.join("tiny2.bin"), &[1u8; 10]);
        let g = find(&opts(t), &AtomicBool::new(false), &DupeProgress::default());
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!(g[0].files.len(), 3);
        assert_eq!(g[0].wasted(), 10_000);
        assert!(g[0].files.iter().all(|f| !f.path.ends_with("different.bin")));
    }

    #[test]
    fn large_files_that_differ_only_in_the_middle_are_told_apart_by_the_full_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let a = vec![3u8; 600_000];
        let mut b = a.clone();
        b[300_000] = 9; // outside the first and last 64 KiB: quick hash cannot see it
        write(&t.join("a.bin"), &a);
        write(&t.join("b.bin"), &b);
        write(&t.join("c.bin"), &a);
        let g = find(&opts(t), &AtomicBool::new(false), &DupeProgress::default());
        assert_eq!(g.len(), 1);
        let names: Vec<_> = g[0].files.iter().map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["a.bin", "c.bin"]);
    }

    #[test]
    fn dev_folders_are_skipped_unless_asked() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        write(&t.join("node_modules/x/lib.js"), &[5u8; 4000]);
        write(&t.join("node_modules/y/lib.js"), &[5u8; 4000]);
        write(&t.join("app/intermediates/cxx/lib.so"), &[6u8; 4000]);
        write(&t.join("app/intermediates/cmake/lib.so"), &[6u8; 4000]);
        assert!(find(&opts(t), &AtomicBool::new(false), &DupeProgress::default()).is_empty());
        let mut o = opts(t);
        o.skip_dev = false;
        assert_eq!(find(&o, &AtomicBool::new(false), &DupeProgress::default()).len(), 2, "node_modules pair + intermediates pair");
    }

    #[test]
    fn groups_are_sorted_by_waste_and_keep_rules_work() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        write(&t.join("small/a.bin"), &[1u8; 1000]);
        write(&t.join("small/b.bin"), &[1u8; 1000]);
        write(&t.join("big/a.bin"), &[2u8; 9000]);
        write(&t.join("big/deeper/dir/b.bin"), &[2u8; 9000]);
        let g = find(&opts(t), &AtomicBool::new(false), &DupeProgress::default());
        assert_eq!(g.len(), 2);
        assert!(g[0].wasted() > g[1].wasted());
        let big = &g[0];
        let k = keep_index(big, Keep::ShortestPath);
        assert!(big.files[k].path.ends_with("big/a.bin") || big.files[k].path.ends_with(r"big\a.bin"));
        let del = to_delete(big, k);
        assert_eq!(del.len(), 1);
        assert!(!del.iter().any(|f| f.path == big.files[k].path), "the kept file is never offered for deletion");
        // oldest / newest are decided by mtime
        let mut g2 = big.clone();
        g2.files[0].mtime = 100;
        g2.files[1].mtime = 200;
        assert_eq!(keep_index(&g2, Keep::Oldest), 0);
        assert_eq!(keep_index(&g2, Keep::Newest), 1);
    }

    #[test]
    fn deleting_through_the_cleaner_keeps_one_copy_and_rechecks_changes() {
        use crate::core::cleaner::{clean, CleanOptions};
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        for n in ["a.bin", "b.bin", "c.bin"] {
            write(&t.join(n), &[4u8; 3000]);
        }
        let g = find(&opts(t), &AtomicBool::new(false), &DupeProgress::default());
        let grp = &g[0];
        let del = to_delete(grp, 0);
        let items = to_scan_items(&del);
        // one of the files changes after the scan: it must be skipped, not deleted
        fs::write(&items[0].path, [9u8; 3001]).unwrap();
        let rep = clean(&items, CleanOptions { dry_run: false, permanent: true }, &AtomicBool::new(false), |_, _| {});
        assert!(grp.files[0].path.exists(), "the kept copy is untouched");
        assert!(items[0].path.exists(), "a file that changed since the scan is skipped");
        assert!(!items[1].path.exists());
        assert_eq!(rep.entries.len(), 2);
    }

    #[test]
    fn cancelling_returns_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("a.bin"), &[1u8; 500]);
        write(&tmp.path().join("b.bin"), &[1u8; 500]);
        assert!(find(&opts(tmp.path()), &AtomicBool::new(true), &DupeProgress::default()).is_empty());
    }
}
