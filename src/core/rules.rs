//! Rule engine: decides, without any AI, what a file or folder is and how
//! risky it is to delete. Anything it cannot place is flagged `unknown`.

use crate::config::{home_dir, RulesConfig};
use crate::core::safety::{is_protected_item, norm};
use crate::models::Category;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub category: Category,
    pub rule: &'static str,
    pub reason: String,
    pub unknown: bool,
}

fn v(category: Category, rule: &'static str, reason: &str) -> Option<Verdict> {
    Some(Verdict {
        category,
        rule,
        reason: reason.to_string(),
        unknown: false,
    })
}

fn unknown(rule: &'static str, reason: &str) -> Option<Verdict> {
    Some(Verdict {
        category: Category::Yellow,
        rule,
        reason: reason.to_string(),
        unknown: true,
    })
}

const DAY: i64 = 86_400;

// Only extensions that are never a build artifact. `.dll`/`.so` are deliberately
// absent: system ones live in protected trees, and elsewhere they are build output.
const SYSTEM_EXTS: &[&str] = &["sys", "vxd", "ko"];
const SYSTEM_NAMES: &[&str] = &["pagefile.sys", "hiberfil.sys", "swapfile.sys"];
const IMAGE_EXTS: &[&str] = &["iso", "dmg", "pkg", "msi", "img"];
const ARCHIVE_EXTS: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz"];
const VM_EXTS: &[&str] = &["vhd", "vhdx", "vmdk", "vdi", "qcow2"];
const PY_CACHES: &[&str] = &["__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"];
const CACHE_DIR_NAMES: &[&str] = &[
    "cache",
    "caches",
    "code cache",
    "gpucache",
    "_cacache",
    "crashdumps",
];

/// Folders whose children are temp/cache items by definition.
fn known_locations() -> Vec<String> {
    let mut v: Vec<String> = vec![norm(&std::env::temp_dir())];
    let home = home_dir();
    if cfg!(windows) {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = Path::new(&local).to_path_buf();
            v.push(norm(&local.join("Temp")));
            v.push(norm(&local.join("CrashDumps")));
            v.push(norm(
                &local.join("Microsoft").join("Windows").join("INetCache"),
            ));
        }
        let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        v.push(norm(&Path::new(&sysroot).join("Temp")));
    } else {
        v.push("/tmp".into());
        v.push("/var/tmp".into());
        v.push(norm(&home.join(".cache")));
        if cfg!(target_os = "macos") {
            v.push(norm(&home.join("Library").join("Caches")));
            v.push(norm(&home.join("Library").join("Logs")));
        }
    }
    v.sort();
    v.dedup();
    v
}

pub struct RuleEngine {
    cfg: RulesConfig,
    locations: Vec<String>,
    now: i64,
}

fn name_lc(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// `build/` next to a Gradle build script.
fn is_gradle_build_dir(dir: &Path) -> bool {
    name_lc(dir) == "build"
        && dir
            .parent()
            .map(|p| p.join("build.gradle").is_file() || p.join("build.gradle.kts").is_file())
            .unwrap_or(false)
}

fn ext_lc(path: &Path) -> String {
    path.extension()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

impl RuleEngine {
    pub fn new(cfg: &RulesConfig, now: i64) -> Self {
        Self {
            cfg: cfg.clone(),
            locations: known_locations(),
            now,
        }
    }

    #[cfg(test)]
    fn with_locations(cfg: &RulesConfig, now: i64, locations: Vec<String>) -> Self {
        Self {
            cfg: cfg.clone(),
            locations,
            now,
        }
    }

    fn safe_age(&self) -> i64 {
        self.cfg.safe_age_days * DAY
    }

    fn old_enough(&self, mtime: i64) -> bool {
        self.now - mtime >= self.safe_age()
    }

    fn in_known_location(&self, path: &Path) -> bool {
        let s = norm(path);
        self.locations
            .iter()
            .any(|l| s.starts_with(&format!("{}/", l)))
    }

    fn direct_child_of_location(&self, path: &Path) -> bool {
        path.parent()
            .map(|p| {
                let ps = norm(p);
                self.locations.contains(&ps)
            })
            .unwrap_or(false)
    }

    /// Cheap name/location check: should the scanner treat this folder as ONE
    /// item (sum its size, don't descend)?
    pub fn wants_aggregate(&self, dir: &Path) -> bool {
        let name = name_lc(dir);
        let parent_name = dir.parent().map(name_lc).unwrap_or_default();
        if name == "node_modules"
            || name == ".cxx"
            || PY_CACHES.contains(&name.as_str())
            || CACHE_DIR_NAMES.contains(&name.as_str())
            || (name == "build" && parent_name == "android")
            || is_gradle_build_dir(dir)
        {
            return true;
        }
        if name == "target" {
            return dir
                .parent()
                .map(|p| p.join("Cargo.toml").is_file())
                .unwrap_or(false);
        }
        self.direct_child_of_location(dir)
    }

    /// `None` = nothing worth reporting.
    pub fn classify(&self, path: &Path, size: u64, mtime: i64, is_dir: bool) -> Option<Verdict> {
        if is_protected_item(path) {
            return v(Category::Red, "protected", "Korumalı sistem/kullanıcı yolu");
        }
        if is_dir {
            self.classify_dir(path, mtime)
        } else {
            self.classify_file(path, size, mtime)
        }
    }

    fn classify_dir(&self, path: &Path, mtime: i64) -> Option<Verdict> {
        let name = name_lc(path);
        let parent_name = path.parent().map(name_lc).unwrap_or_default();

        if name == "node_modules" {
            return v(
                Category::Yellow,
                "node_modules",
                "Bağımlılık klasörü; npm/pnpm install ile yeniden oluşur",
            );
        }
        if name == "target"
            && path
                .parent()
                .map(|p| p.join("Cargo.toml").is_file())
                .unwrap_or(false)
        {
            return v(
                Category::Yellow,
                "rust_target",
                "Rust derleme çıktısı; cargo build ile yeniden oluşur",
            );
        }
        if name == ".cxx" {
            return v(
                Category::Yellow,
                "native_build",
                "Yerel (CMake) derleme çıktısı; yeniden üretilir",
            );
        }
        if name == "build" && parent_name == "android" {
            return v(
                Category::Yellow,
                "android_build",
                "Android derleme çıktısı; sonraki derlemede yeniden oluşur",
            );
        }
        if is_gradle_build_dir(path) {
            return v(
                Category::Yellow,
                "gradle_build",
                "Gradle derleme çıktısı; sonraki derlemede yeniden oluşur",
            );
        }
        if name == "caches" && parent_name == ".gradle" {
            return v(
                Category::Yellow,
                "gradle_cache",
                "Gradle önbelleği; bağımlılıklar yeniden indirilir",
            );
        }
        if PY_CACHES.contains(&name.as_str()) {
            return v(Category::Green, "py_cache", "Python önbelleği");
        }
        let green_dir =
            CACHE_DIR_NAMES.contains(&name.as_str()) || self.direct_child_of_location(path);
        if green_dir && self.old_enough(mtime) {
            return v(
                Category::Green,
                "cache_dir",
                "Önbellek/geçici klasör, uzun süredir değişmemiş",
            );
        }
        None
    }

    fn classify_file(&self, path: &Path, size: u64, mtime: i64) -> Option<Verdict> {
        let name = name_lc(path);
        let ext = ext_lc(path);
        let large = size >= self.cfg.large_file_mb * 1024 * 1024;

        if norm(path).contains("/appdata/local/programs/") {
            return v(
                Category::Red,
                "installed_app",
                "Kurulu uygulama dosyası; silmek uygulamayı bozar",
            );
        }
        if SYSTEM_NAMES.contains(&name.as_str()) || SYSTEM_EXTS.contains(&ext.as_str()) {
            return v(Category::Red, "system_file", "Sistem/kütüphane dosyası");
        }
        if large && self.now - mtime < DAY {
            return v(
                Category::Red,
                "recently_modified",
                "Son 24 saatte değişmiş; kullanımda olabilir",
            );
        }
        if self
            .cfg
            .safe_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&ext))
        {
            return if self.old_enough(mtime) {
                v(
                    Category::Green,
                    "safe_ext",
                    "Geçici/log/önbellek uzantısı, uzun süredir değişmemiş",
                )
            } else {
                None
            };
        }
        if self.in_known_location(path) {
            return if self.old_enough(mtime) {
                v(
                    Category::Green,
                    "temp_location",
                    "Bilinen geçici/önbellek konumunda, eski",
                )
            } else {
                None
            };
        }
        if VM_EXTS.contains(&ext.as_str()) {
            return unknown(
                "vm_disk",
                "Sanal makine diski; hâlâ kullanılıp kullanılmadığını doğrula",
            );
        }
        if IMAGE_EXTS.contains(&ext.as_str()) && self.old_enough(mtime) {
            return v(
                Category::Yellow,
                "installer_image",
                "Kurulum/disk imajı; kurulum bittiyse gereksiz olabilir",
            );
        }
        if ARCHIVE_EXTS.contains(&ext.as_str()) && self.old_enough(mtime) {
            return unknown("archive", "Eski arşiv; içeriğini kontrol et");
        }
        if large {
            return unknown("large_file", "Büyük dosya; ne olduğunu doğrula");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_800_000_000;
    const MB: u64 = 1024 * 1024;

    fn engine() -> RuleEngine {
        RuleEngine::with_locations(&RulesConfig::default(), NOW, vec!["/scratch/tmp".into()])
    }

    fn ago(days: i64) -> i64 {
        NOW - days * DAY
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn old_log_is_green_recent_log_is_ignored() {
        let e = engine();
        let g = e
            .classify(&p("/data/app.log"), 5 * MB, ago(90), false)
            .unwrap();
        assert_eq!((g.category, g.rule), (Category::Green, "safe_ext"));
        assert!(e
            .classify(&p("/data/app.log"), 5 * MB, ago(2), false)
            .is_none());
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        let g = engine()
            .classify(&p("/data/CRASH.DMP"), 5 * MB, ago(90), false)
            .unwrap();
        assert_eq!(g.category, Category::Green);
    }

    #[test]
    fn known_location_file_needs_age() {
        let e = engine();
        assert_eq!(
            e.classify(&p("/scratch/tmp/blob.bin"), 3 * MB, ago(60), false)
                .unwrap()
                .rule,
            "temp_location"
        );
        assert!(e
            .classify(&p("/scratch/tmp/blob.bin"), 3 * MB, ago(1), false)
            .is_none());
    }

    #[test]
    fn large_unknown_file_is_yellow_and_flagged_for_ai() {
        let g = engine()
            .classify(&p("/data/blob.bin"), 800 * MB, ago(90), false)
            .unwrap();
        assert_eq!(g.category, Category::Yellow);
        assert!(g.unknown);
    }

    #[test]
    fn small_unknown_file_is_ignored() {
        assert!(engine()
            .classify(&p("/data/notes.txt"), 2 * MB, ago(400), false)
            .is_none());
    }

    #[test]
    fn system_files_and_fresh_large_files_are_red() {
        let e = engine();
        assert_eq!(
            e.classify(&p("/data/pagefile.sys"), 9000 * MB, ago(1000), false)
                .unwrap()
                .category,
            Category::Red
        );
        assert_eq!(
            e.classify(&p("/data/driver.sys"), 2 * MB, ago(1000), false)
                .unwrap()
                .category,
            Category::Red
        );
        // build artifacts like .so/.dll outside system trees are NOT system files
        assert!(e
            .classify(&p("/proj/out/libfoo.so"), 2 * MB, ago(1000), false)
            .is_none());
        let fresh = e
            .classify(&p("/data/render.mov"), 900 * MB, NOW - 3600, false)
            .unwrap();
        assert_eq!(
            (fresh.category, fresh.rule),
            (Category::Red, "recently_modified")
        );
    }

    #[test]
    fn files_of_installed_apps_are_red() {
        let g = engine()
            .classify(
                &p("C:/Users/x/AppData/Local/Programs/Ollama/lib/big.dll"),
                900 * MB,
                ago(90),
                false,
            )
            .unwrap();
        assert_eq!((g.category, g.rule), (Category::Red, "installed_app"));
    }

    #[test]
    fn vm_disk_and_installer_images() {
        let e = engine();
        let vm = e
            .classify(&p("/vm/disk.vhdx"), 4000 * MB, ago(1), false)
            .unwrap();
        assert!(vm.unknown);
        assert_eq!(
            e.classify(&p("/dl/os.iso"), 300 * MB, ago(100), false)
                .unwrap()
                .rule,
            "installer_image"
        );
    }

    #[test]
    fn dev_dirs_are_yellow_python_caches_green() {
        let e = engine();
        assert_eq!(
            e.classify(&p("/proj/node_modules"), 100 * MB, ago(1), true)
                .unwrap()
                .category,
            Category::Yellow
        );
        assert_eq!(
            e.classify(&p("/app/android/build"), 100 * MB, ago(1), true)
                .unwrap()
                .rule,
            "android_build"
        );
        assert_eq!(
            e.classify(&p("/proj/.gradle/caches"), 100 * MB, ago(1), true)
                .unwrap()
                .rule,
            "gradle_cache"
        );
        assert_eq!(
            e.classify(&p("/proj/__pycache__"), 2 * MB, ago(0), true)
                .unwrap()
                .category,
            Category::Green
        );
        // a plain "build" dir that is not under android/ is not special
        assert!(e
            .classify(&p("/proj/build"), 100 * MB, ago(1), true)
            .is_none());
    }

    #[test]
    fn green_cache_dir_needs_age() {
        let e = engine();
        assert!(e.classify(&p("/x/Cache"), 10 * MB, ago(1), true).is_none());
        assert_eq!(
            e.classify(&p("/x/Cache"), 10 * MB, ago(45), true)
                .unwrap()
                .category,
            Category::Green
        );
    }

    #[test]
    fn aggregation_hints() {
        let e = engine();
        assert!(e.wants_aggregate(&p("/proj/node_modules")));
        assert!(e.wants_aggregate(&p("/scratch/tmp/some-installer")));
        assert!(!e.wants_aggregate(&p("/proj/src")));
        assert!(!e.wants_aggregate(&p("/app/build")));
        assert!(e.wants_aggregate(&p("/app/android/build")));
    }

    #[test]
    fn protected_home_is_red() {
        let g = engine().classify(&home_dir(), 1, 0, true).unwrap();
        assert_eq!(g.category, Category::Red);
    }
}
