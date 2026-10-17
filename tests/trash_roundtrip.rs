//! Needs a desktop session with a Trash/Recycle Bin, so it is opt-in:
//! `cargo test --test trash_roundtrip -- --ignored`

use draxbyte::core::cleaner::{clean, undo, CleanOptions};
use draxbyte::core::scanner::mtime_secs;
use draxbyte::models::{Category, Outcome, ScanItem};
use std::sync::atomic::AtomicBool;

#[test]
#[ignore = "touches the real Trash"]
fn trashed_file_can_be_restored() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("draxbyte-roundtrip.tmp");
    std::fs::write(&f, b"roundtrip").unwrap();
    let meta = std::fs::metadata(&f).unwrap();
    let item = ScanItem {
        path: f.clone(),
        size: meta.len(),
        mtime: mtime_secs(&meta),
        is_dir: false,
        category: Category::Green,
        rule: "t".into(),
        reason: "t".into(),
        unknown: false,
    };

    let report = clean(
        &[item],
        CleanOptions::default(),
        &AtomicBool::new(false),
        |_, _| {},
    );
    assert_eq!(
        report.entries[0].outcome,
        Outcome::Trashed,
        "{:?}",
        report.entries[0]
    );
    assert!(!f.exists(), "file should be in the Trash");

    assert_eq!(undo(&report).unwrap(), 1);
    assert_eq!(std::fs::read(&f).unwrap(), b"roundtrip");
}
