//! Disk map: how big every folder is, as a tree with bounded memory, plus the squarified
//! treemap layout that turns one level of that tree into rectangles.
//!
//! Only the big parts are kept: each folder keeps its largest children and folds the rest
//! (small files, small folders) into one "other" entry, so mapping a whole drive stays small.

use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// Files below (or 1 for a file).
    pub files: u64,
    pub is_dir: bool,
    /// Largest first. For a folder: kept files/folders, then the folded "other" entry.
    pub children: Vec<Node>,
    /// The folded remainder (small files and folders); not a real path.
    pub other: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MapOptions {
    /// Children smaller than this are folded into "other".
    pub min_keep: u64,
    /// At most this many children are kept per folder.
    pub max_children: usize,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self { min_keep: 8 << 20, max_children: 64 }
    }
}

#[derive(Default)]
pub struct MapProgress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Builds the size tree of `root`. Symlinks / junctions are not followed.
pub fn build(root: &Path, opts: &MapOptions, cancel: &AtomicBool, progress: &MapProgress) -> Node {
    walk(root, 0, opts, cancel, progress)
}

fn walk(dir: &Path, depth: usize, opts: &MapOptions, cancel: &AtomicBool, progress: &MapProgress) -> Node {
    let mut node = Node { name: name_of(dir), path: dir.to_path_buf(), size: 0, files: 0, is_dir: true, children: Vec::new(), other: false };
    if cancel.load(Relaxed) {
        return node;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        progress.errors.fetch_add(1, Relaxed);
        return node;
    };
    let mut subdirs: Vec<PathBuf> = Vec::new();
    let (mut folded_size, mut folded_files, mut folded_items) = (0u64, 0u64, 0u64);
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            subdirs.push(e.path());
        } else if ft.is_file() {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            let n = progress.files.fetch_add(1, Relaxed);
            progress.bytes.fetch_add(size, Relaxed);
            if n % 2048 == 0 {
                if let Ok(mut c) = progress.current.try_lock() {
                    *c = dir.to_string_lossy().into_owned();
                }
            }
            node.size += size;
            node.files += 1;
            if size >= opts.min_keep {
                node.children.push(Node { name: e.file_name().to_string_lossy().into_owned(), path: e.path(), size, files: 1, is_dir: false, children: Vec::new(), other: false });
            } else {
                folded_size += size;
                folded_files += 1;
                folded_items += 1;
            }
        }
    }
    // Sub-folders: in parallel near the top, one after another deeper down.
    let subs: Vec<Node> = if depth < 3 {
        subdirs.par_iter().map(|p| walk(p, depth + 1, opts, cancel, progress)).collect()
    } else {
        subdirs.iter().map(|p| walk(p, depth + 1, opts, cancel, progress)).collect()
    };
    for s in subs {
        node.size += s.size;
        node.files += s.files;
        if s.size >= opts.min_keep {
            node.children.push(s);
        } else {
            folded_size += s.size;
            folded_files += s.files;
            folded_items += 1;
        }
    }
    node.children.sort_by(|a, b| b.size.cmp(&a.size));
    if node.children.len() > opts.max_children {
        for c in node.children.split_off(opts.max_children) {
            folded_size += c.size;
            folded_files += c.files;
            folded_items += 1;
        }
    }
    if folded_size > 0 {
        node.children.push(Node {
            name: crate::tf!("{} küçük öğe", "{} small items", folded_items),
            path: dir.to_path_buf(),
            size: folded_size,
            files: folded_files,
            is_dir: false,
            children: Vec::new(),
            other: true,
        });
    }
    node
}

// ---- treemap layout ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn area(&self) -> f32 {
        self.w * self.h
    }
}

fn worst_ratio(row: &[f64], sum: f64, side: f64) -> f64 {
    let mut worst = 0.0f64;
    for &r in row {
        let a = (side * side * r) / (sum * sum);
        let b = (sum * sum) / (side * side * r);
        worst = worst.max(a.max(b));
    }
    worst
}

/// Squarified treemap (Bruls, Huizing, van Wijk): one rectangle per value, areas proportional to
/// the values, tiles as close to square as possible. `values` should be sorted largest first;
/// non-positive values get an empty rectangle.
pub fn squarify(values: &[u64], bounds: Rect) -> Vec<Rect> {
    let n = values.len();
    let mut out = vec![Rect::default(); n];
    let total: f64 = values.iter().map(|&v| v as f64).sum();
    if n == 0 || total <= 0.0 || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return out;
    }
    let scale = (bounds.w as f64 * bounds.h as f64) / total;
    let areas: Vec<f64> = values.iter().map(|&v| v as f64 * scale).collect();
    let (mut x, mut y, mut w, mut h) = (bounds.x as f64, bounds.y as f64, bounds.w as f64, bounds.h as f64);
    let mut i = 0;
    while i < n {
        if areas[i] <= 0.0 {
            i += 1;
            continue;
        }
        let side = w.min(h);
        let mut end = i + 1;
        let mut sum = areas[i];
        let mut best = worst_ratio(&areas[i..end], sum, side);
        while end < n && areas[end] > 0.0 {
            let new_sum = sum + areas[end];
            let nw = worst_ratio(&areas[i..=end], new_sum, side);
            if nw <= best {
                end += 1;
                sum = new_sum;
                best = nw;
            } else {
                break;
            }
        }
        if w >= h {
            let rw = sum / h;
            let mut yy = y;
            for k in i..end {
                let rh = areas[k] / rw;
                out[k] = Rect { x: x as f32, y: yy as f32, w: rw as f32, h: rh as f32 };
                yy += rh;
            }
            x += rw;
            w -= rw;
        } else {
            let rh = sum / w;
            let mut xx = x;
            for k in i..end {
                let rw = areas[k] / rh;
                out[k] = Rect { x: xx as f32, y: y as f32, w: rw as f32, h: rh as f32 };
                xx += rw;
            }
            y += rh;
            h -= rh;
        }
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn overlap(a: &Rect, b: &Rect) -> f32 {
        let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }

    #[test]
    fn squarify_fills_the_bounds_proportionally_without_overlap() {
        let values = [600u64, 300, 200, 150, 100, 50, 40, 30, 20, 10, 5, 1];
        let b = Rect { x: 10.0, y: 20.0, w: 800.0, h: 500.0 };
        let r = squarify(&values, b);
        let total: u64 = values.iter().sum();
        let mut area_sum = 0.0;
        for (i, rect) in r.iter().enumerate() {
            assert!(rect.x >= b.x - 0.01 && rect.y >= b.y - 0.01, "tile {i} outside");
            assert!(rect.x + rect.w <= b.x + b.w + 0.01 && rect.y + rect.h <= b.y + b.h + 0.01, "tile {i} outside");
            let want = b.area() * values[i] as f32 / total as f32;
            assert!((rect.area() - want).abs() <= want * 0.01 + 0.05, "tile {i}: {} vs {}", rect.area(), want);
            area_sum += rect.area();
        }
        assert!((area_sum - b.area()).abs() < b.area() * 0.001);
        for i in 0..r.len() {
            for j in (i + 1)..r.len() {
                assert!(overlap(&r[i], &r[j]) < 0.5, "tiles {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn squarify_tiles_are_reasonably_square() {
        let values: Vec<u64> = (1..=40).rev().map(|v| v * 10).collect();
        let r = squarify(&values, Rect { x: 0.0, y: 0.0, w: 900.0, h: 600.0 });
        let big = &r[..10];
        for t in big {
            let ratio = (t.w / t.h).max(t.h / t.w);
            assert!(ratio < 4.0, "big tile too elongated: {ratio}");
        }
    }

    #[test]
    fn squarify_edge_cases() {
        assert!(squarify(&[], Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 }).is_empty());
        let one = squarify(&[5], Rect { x: 0.0, y: 0.0, w: 10.0, h: 4.0 });
        assert_eq!(one[0], Rect { x: 0.0, y: 0.0, w: 10.0, h: 4.0 });
        let with_zero = squarify(&[10, 0, 5], Rect { x: 0.0, y: 0.0, w: 30.0, h: 10.0 });
        assert_eq!(with_zero[1].area(), 0.0);
        assert!(with_zero[0].area() > 0.0 && with_zero[2].area() > 0.0);
        assert!(squarify(&[1, 2], Rect { x: 0.0, y: 0.0, w: 0.0, h: 5.0 }).iter().all(|r| r.area() == 0.0));
    }

    #[test]
    fn tree_sizes_add_up_and_small_things_are_folded() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        fs::create_dir_all(t.join("big/inner")).unwrap();
        fs::create_dir_all(t.join("small")).unwrap();
        fs::write(t.join("big/a.bin"), vec![0u8; 3000]).unwrap();
        fs::write(t.join("big/inner/b.bin"), vec![0u8; 2000]).unwrap();
        fs::write(t.join("small/c.txt"), vec![0u8; 10]).unwrap();
        fs::write(t.join("root.bin"), vec![0u8; 500]).unwrap();
        let opts = MapOptions { min_keep: 1000, max_children: 8 };
        let n = build(t, &opts, &AtomicBool::new(false), &MapProgress::default());
        assert_eq!(n.size, 3000 + 2000 + 10 + 500);
        assert_eq!(n.files, 4);
        // kept: big (5000); folded: small (10) + root.bin (500) as "other"
        assert_eq!(n.children[0].name, "big");
        assert_eq!(n.children[0].size, 5000);
        assert_eq!(n.children[0].children[0].name, "a.bin");
        let other = n.children.iter().find(|c| c.other).expect("folded entry");
        assert_eq!(other.size, 510);
        // children never exceed the parent
        let kept: u64 = n.children.iter().map(|c| c.size).sum();
        assert_eq!(kept, n.size);
    }

    #[test]
    fn child_count_is_capped_and_the_rest_is_folded() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(tmp.path().join(format!("f{i:02}.bin")), vec![0u8; 2000 + i]).unwrap();
        }
        let opts = MapOptions { min_keep: 1000, max_children: 5 };
        let n = build(tmp.path(), &opts, &AtomicBool::new(false), &MapProgress::default());
        assert_eq!(n.children.len(), 6, "5 kept + 1 folded");
        assert!(n.children.last().unwrap().other);
        assert_eq!(n.children.iter().map(|c| c.size).sum::<u64>(), n.size);
    }

    #[test]
    fn cancel_stops_the_walk() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), b"x").unwrap();
        let n = build(tmp.path(), &MapOptions::default(), &AtomicBool::new(true), &MapProgress::default());
        assert_eq!(n.size, 0);
    }
}
