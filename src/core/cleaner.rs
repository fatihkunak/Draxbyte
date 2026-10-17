//! Deleting: Trash by default, permanent only on request, dry-run never touches disk.

use crate::core::safety::pre_delete_check;
use crate::models::{CleanEntry, CleanReport, Outcome, ScanItem};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

#[derive(Debug, Clone, Copy, Default)]
pub struct CleanOptions {
    pub dry_run: bool,
    /// Skip the Trash. Needs its own confirmation in the UI.
    pub permanent: bool,
}

/// `on_progress(done, total)` is called after every item.
pub fn clean(
    items: &[ScanItem],
    opts: CleanOptions,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(usize, usize),
) -> CleanReport {
    let mut report = CleanReport {
        dry_run: opts.dry_run,
        permanent: opts.permanent,
        ..Default::default()
    };
    let total = items.len();

    for (i, item) in items.iter().enumerate() {
        if cancel.load(Relaxed) {
            report.entries.push(CleanEntry {
                path: item.path.clone(),
                size: item.size,
                outcome: Outcome::Skipped("iptal edildi".into()),
            });
            continue;
        }
        let outcome = match pre_delete_check(item) {
            Err(skip) => Outcome::Skipped(skip.message().to_string()),
            Ok(()) if opts.dry_run => Outcome::DryRun,
            Ok(()) => delete_one(item, opts.permanent),
        };
        if matches!(
            outcome,
            Outcome::DryRun | Outcome::Trashed | Outcome::Deleted
        ) {
            report.freed_bytes += item.size;
        }
        report.entries.push(CleanEntry {
            path: item.path.clone(),
            size: item.size,
            outcome,
        });
        on_progress(i + 1, total);
    }
    report
}

fn delete_one(item: &ScanItem, permanent: bool) -> Outcome {
    if permanent {
        let r = if item.is_dir {
            std::fs::remove_dir_all(&item.path)
        } else {
            std::fs::remove_file(&item.path)
        };
        match r {
            Ok(()) => Outcome::Deleted,
            Err(e) => Outcome::Failed(e.to_string()),
        }
    } else {
        match trash::delete(&item.path) {
            Ok(()) => Outcome::Trashed,
            Err(e) => Outcome::Failed(e.to_string()),
        }
    }
}

/// Put back what the given report moved to the Trash. Returns how many were restored.
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
pub fn undo(report: &CleanReport) -> anyhow::Result<usize> {
    use trash::os_limited::{list, restore_all};
    let wanted: std::collections::HashSet<_> = report
        .entries
        .iter()
        .filter(|e| e.outcome == Outcome::Trashed)
        .map(|e| e.path.clone())
        .collect();
    if wanted.is_empty() {
        return Ok(0);
    }
    let candidates: Vec<_> = list()?
        .into_iter()
        .filter(|t| wanted.contains(&t.original_path()))
        .collect();
    let n = candidates.len();
    restore_all(candidates)?;
    Ok(n)
}

#[cfg(target_os = "macos")]
pub fn undo(_report: &CleanReport) -> anyhow::Result<usize> {
    anyhow::bail!("macOS'ta geri alma desteklenmiyor; dosyaları Çöp Kutusu'ndan elle geri al")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scanner::mtime_secs;
    use crate::models::Category;
    use std::path::Path;

    fn item_for(path: &Path, category: Category) -> ScanItem {
        let meta = std::fs::metadata(path).unwrap();
        ScanItem {
            path: path.to_path_buf(),
            size: meta.len(),
            mtime: mtime_secs(&meta),
            is_dir: meta.is_dir(),
            category,
            rule: "t".into(),
            reason: "t".into(),
            unknown: false,
        }
    }

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn dry_run_touches_nothing_but_reports_size() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.tmp");
        std::fs::write(&f, b"12345").unwrap();
        let rep = clean(
            &[item_for(&f, Category::Green)],
            CleanOptions {
                dry_run: true,
                permanent: true,
            },
            &no_cancel(),
            |_, _| {},
        );
        assert!(f.exists());
        assert_eq!(rep.freed_bytes, 5);
        assert_eq!(rep.entries[0].outcome, Outcome::DryRun);
    }

    #[test]
    fn permanent_delete_removes_file_and_dir() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.tmp");
        std::fs::write(&f, b"12345").unwrap();
        let d = dir.path().join("sub");
        std::fs::create_dir_all(d.join("inner")).unwrap();
        std::fs::write(d.join("inner/x"), b"xx").unwrap();
        let items = [
            item_for(&f, Category::Green),
            item_for(&d, Category::Yellow),
        ];
        let rep = clean(
            &items,
            CleanOptions {
                dry_run: false,
                permanent: true,
            },
            &no_cancel(),
            |_, _| {},
        );
        assert!(!f.exists() && !d.exists());
        assert_eq!(rep.count(|o| *o == Outcome::Deleted), 2);
    }

    #[test]
    fn red_items_and_changed_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.tmp");
        std::fs::write(&f, b"12345").unwrap();
        let red = item_for(&f, Category::Red);
        let mut stale = item_for(&f, Category::Green);
        stale.size += 1;
        let rep = clean(
            &[red, stale],
            CleanOptions {
                dry_run: false,
                permanent: true,
            },
            &no_cancel(),
            |_, _| {},
        );
        assert!(f.exists());
        assert_eq!(rep.freed_bytes, 0);
        assert_eq!(rep.count(|o| matches!(o, Outcome::Skipped(_))), 2);
    }

    #[test]
    fn dir_with_newer_file_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("cachedir");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("old"), b"x").unwrap();
        let mut it = item_for(&d, Category::Yellow);
        it.mtime -= 1000; // pretend the scan saw an older tree
        let rep = clean(
            &[it],
            CleanOptions {
                dry_run: false,
                permanent: true,
            },
            &no_cancel(),
            |_, _| {},
        );
        assert!(d.exists());
        assert!(matches!(rep.entries[0].outcome, Outcome::Skipped(_)));
    }

    #[test]
    fn cancel_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.tmp");
        std::fs::write(&f, b"1").unwrap();
        let rep = clean(
            &[item_for(&f, Category::Green)],
            CleanOptions {
                dry_run: false,
                permanent: true,
            },
            &AtomicBool::new(true),
            |_, _| {},
        );
        assert!(f.exists());
        assert!(matches!(rep.entries[0].outcome, Outcome::Skipped(_)));
    }
}
