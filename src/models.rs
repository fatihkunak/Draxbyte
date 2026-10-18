//! Shared data structures.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    /// Safe to delete (cache, temp, log).
    Green,
    /// Be careful (large or old but unknown, regenerable build output).
    Yellow,
    /// Do not delete (system file, probably in use).
    Red,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Green => "green",
            Category::Yellow => "yellow",
            Category::Red => "red",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "green" => Some(Category::Green),
            "yellow" => Some(Category::Yellow),
            "red" => Some(Category::Red),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Green => crate::i18n::t("Güvenli", "Safe"),
            Category::Yellow => crate::i18n::t("Dikkat", "Careful"),
            Category::Red => crate::i18n::t("Silme", "Keep"),
        }
    }
}

/// What kind of thing an item is, for the storage-distribution charts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Video,
    Image,
    Audio,
    Document,
    Archive,
    Developer,
    Cache,
    VmDisk,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Video,
        Kind::Image,
        Kind::Audio,
        Kind::Document,
        Kind::Archive,
        Kind::Developer,
        Kind::Cache,
        Kind::VmDisk,
        Kind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => crate::i18n::t("Videolar", "Videos"),
            Kind::Image => crate::i18n::t("Resimler", "Images"),
            Kind::Audio => crate::i18n::t("Müzik", "Music"),
            Kind::Document => crate::i18n::t("Belgeler", "Documents"),
            Kind::Archive => crate::i18n::t("Arşiv / kurulum", "Archives"),
            Kind::Developer => crate::i18n::t("Derleme çıktısı", "Build output"),
            Kind::Cache => crate::i18n::t("Önbellek / geçici", "Cache / temp"),
            Kind::VmDisk => crate::i18n::t("Sanal diskler", "Virtual disks"),
            Kind::Other => crate::i18n::t("Diğer", "Other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanItem {
    pub path: PathBuf,
    pub size: u64,
    /// Unix seconds. For directories: the newest descendant's mtime.
    pub mtime: i64,
    pub is_dir: bool,
    pub category: Category,
    /// Short rule id, e.g. `safe_ext`, `node_modules`.
    pub rule: String,
    pub reason: String,
    /// The rule engine could not tell what this is (candidate for "ask AI").
    pub unknown: bool,
}

impl ScanItem {
    pub fn age_days(&self, now: i64) -> i64 {
        ((now - self.mtime).max(0)) / 86_400
    }

    pub fn kind(&self) -> Kind {
        match self.rule.as_str() {
            "node_modules" | "rust_target" | "native_build" | "android_build" | "gradle_build"
            | "gradle_cache" => return Kind::Developer,
            "py_cache" | "cache_dir" | "temp_location" | "safe_ext" | "installer_cache"
            | "windows_old" => return Kind::Cache,
            "vm_disk" => return Kind::VmDisk,
            _ => {}
        }
        let ext = self
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "flv" | "m4v" => Kind::Video,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "raw"
            | "psd" => Kind::Image,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => Kind::Audio,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "odt" | "csv" => {
                Kind::Document
            }
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "iso" | "dmg" | "pkg"
            | "msi" | "img" | "exe" => Kind::Archive,
            "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" => Kind::VmDisk,
            _ => Kind::Other,
        }
    }

    /// Cache key: path + size + mtime, so a changed file never gets a stale answer.
    pub fn ai_key(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.path.to_string_lossy().as_bytes());
        h.update(b"|");
        h.update(self.size.to_le_bytes());
        h.update(b"|");
        h.update(self.mtime.to_le_bytes());
        hex::encode(h.finalize())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanSummary {
    pub session_id: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub roots: Vec<String>,
    pub item_count: u64,
    pub total_bytes: u64,
    pub green_bytes: u64,
    pub files_visited: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    DryRun,
    Trashed,
    Deleted,
    Skipped(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct CleanEntry {
    pub path: PathBuf,
    pub size: u64,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Default)]
pub struct CleanReport {
    pub entries: Vec<CleanEntry>,
    pub freed_bytes: u64,
    pub dry_run: bool,
    pub permanent: bool,
}

impl CleanReport {
    pub fn count(&self, f: impl Fn(&Outcome) -> bool) -> usize {
        self.entries.iter().filter(|e| f(&e.outcome)).count()
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", b)
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(size: u64, mtime: i64) -> ScanItem {
        ScanItem {
            path: PathBuf::from("/tmp/a.bin"),
            size,
            mtime,
            is_dir: false,
            category: Category::Yellow,
            rule: "x".into(),
            reason: "y".into(),
            unknown: true,
        }
    }

    #[test]
    fn ai_key_changes_with_size_or_mtime() {
        let a = item(10, 100).ai_key();
        assert_eq!(a, item(10, 100).ai_key());
        assert_ne!(a, item(11, 100).ai_key());
        assert_ne!(a, item(10, 101).ai_key());
    }

    #[test]
    fn kind_comes_from_rule_then_extension() {
        let mut i = item(1, 1);
        i.rule = "node_modules".into();
        assert_eq!(i.kind(), Kind::Developer);
        i.rule = "large_file".into();
        i.path = PathBuf::from("/x/clip.MP4");
        assert_eq!(i.kind(), Kind::Video);
        i.path = PathBuf::from("/x/blob.bin");
        assert_eq!(i.kind(), Kind::Other);
        i.path = PathBuf::from("/x/old.log");
        i.rule = "safe_ext".into();
        assert_eq!(i.kind(), Kind::Cache);
    }

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn category_roundtrip() {
        for c in [Category::Green, Category::Yellow, Category::Red] {
            assert_eq!(Category::parse(c.as_str()), Some(c));
        }
    }
}
