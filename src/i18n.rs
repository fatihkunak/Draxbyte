//! Turkish / English UI text. The current language is a process-wide setting so that
//! core types (categories, errors) and the UI agree without threading a handle everywhere.
//!
//! Usage: `t("Tarama", "Scan")` for literals, `tr!(format!(..), format!(..))` for formatted text.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Tr,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(1);

pub fn set(l: Lang) {
    CURRENT.store(if l == Lang::Tr { 0 } else { 1 }, Ordering::Relaxed);
}

pub fn get() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 0 {
        Lang::Tr
    } else {
        Lang::En
    }
}

pub fn is_en() -> bool {
    get() == Lang::En
}

/// Picks the text for the current language.
pub fn t(tr: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        tr
    }
}

/// Like `t` for text that needs formatting: both arguments are evaluated lazily.
#[macro_export]
macro_rules! tr {
    ($tr:expr, $en:expr) => {
        if $crate::i18n::is_en() {
            $en
        } else {
            $tr
        }
    };
}

/// Formatted text in both languages: `tf!("{} dk", "{} min", n)`. The templates must be literals
/// (they are checked by `format!`); named captures like `{n}` work as usual.
#[macro_export]
macro_rules! tf {
    ($tr:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($tr $(, $arg)*)
        }
    };
}

/// The word the user types to confirm a permanent delete.
pub fn confirm_word() -> &'static str {
    t("SIL", "DELETE")
}

/// `"auto"` follows the system language; `"tr"` / `"en"` force one.
pub fn resolve(setting: &str) -> Lang {
    match setting.trim().to_lowercase().as_str() {
        "tr" | "turkish" | "türkçe" => Lang::Tr,
        "en" | "english" => Lang::En,
        _ => system_language(),
    }
}

/// Language tag such as `tr-TR` of the user's locale, if it can be read.
fn system_locale() -> Option<String> {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
        }
        let mut buf = [0u16; 85];
        // SAFETY: the buffer is 85 u16s (LOCALE_NAME_MAX_LENGTH) and the API writes at most `len`.
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            return Some(String::from_utf16_lossy(&buf[..(n as usize - 1)]));
        }
        None
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty() && v != "C" && v != "POSIX")
        })
    }
}

pub fn system_language() -> Lang {
    match system_locale() {
        Some(l) if l.to_lowercase().starts_with("tr") => Lang::Tr,
        _ => Lang::En,
    }
}

/// Human text for a rule id. Scans store the (Turkish) reason string, so the UI maps the
/// rule id instead and always speaks the current language.
pub fn rule_reason(rule: &str, stored: &str) -> String {
    let (tr, en) = match rule {
        "protected" => (
            "Korumalı sistem/kullanıcı yolu",
            "Protected system/user path",
        ),
        "installed_app" => (
            "Kurulu uygulama dosyası; silmek uygulamayı bozar",
            "Installed application file; deleting it breaks the app",
        ),
        "system_file" => ("Sistem/kütüphane dosyası", "System file"),
        "recently_modified" => (
            "Son 24 saatte değişmiş; kullanımda olabilir",
            "Changed in the last 24 h; may be in use",
        ),
        "safe_ext" => (
            "Geçici/log/önbellek uzantısı, uzun süredir değişmemiş",
            "Temp/log/cache extension, untouched for a long time",
        ),
        "temp_location" => (
            "Bilinen geçici/önbellek konumunda, eski",
            "In a known temp/cache location, old",
        ),
        "vm_disk" => (
            "Sanal makine diski; hâlâ kullanılıp kullanılmadığını doğrula",
            "Virtual machine disk; check it is still needed",
        ),
        "installer_image" => (
            "Kurulum/disk imajı; kurulum bittiyse gereksiz olabilir",
            "Installer/disk image; probably not needed after installing",
        ),
        "archive" => (
            "Eski arşiv; içeriğini kontrol et",
            "Old archive; check what is inside",
        ),
        "large_file" => (
            "Büyük dosya; ne olduğunu doğrula",
            "Large file; check what it is",
        ),
        "node_modules" => (
            "Bağımlılık klasörü; npm/pnpm install ile yeniden oluşur",
            "Dependency folder; recreated by npm/pnpm install",
        ),
        "rust_target" => (
            "Rust derleme çıktısı; cargo build ile yeniden oluşur",
            "Rust build output; recreated by cargo build",
        ),
        "native_build" => (
            "Yerel (CMake) derleme çıktısı; yeniden üretilir",
            "Native (CMake) build output; regenerated",
        ),
        "android_build" | "gradle_build" => (
            "Gradle derleme çıktısı; sonraki derlemede yeniden oluşur",
            "Gradle build output; regenerated on the next build",
        ),
        "gradle_cache" => (
            "Gradle önbelleği; bağımlılıklar yeniden indirilir",
            "Gradle cache; dependencies are downloaded again",
        ),
        "py_cache" => ("Python önbelleği", "Python cache"),
        "cache_dir" => (
            "Önbellek/geçici klasör, uzun süredir değişmemiş",
            "Cache/temp folder, untouched for a long time",
        ),
        _ => return stored.to_string(),
    };
    t(tr, en).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_forces_or_follows_system() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(resolve("tr"), Lang::Tr);
        assert_eq!(resolve("EN"), Lang::En);
        // "auto" and junk fall back to the system language (either value is fine, it must not panic)
        let _ = resolve("auto");
        let _ = resolve("xx");
    }

    #[test]
    fn t_and_tr_follow_the_current_language() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set(Lang::Tr);
        assert_eq!(t("Tarama", "Scan"), "Tarama");
        assert_eq!(tr!(format!("{} öğe", 2), format!("{} items", 2)), "2 öğe");
        set(Lang::En);
        assert_eq!(t("Tarama", "Scan"), "Scan");
        assert_eq!(tr!(format!("{} öğe", 2), format!("{} items", 2)), "2 items");
    }

    #[test]
    fn rule_reason_is_translated_and_unknown_rules_keep_the_stored_text() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set(Lang::En);
        assert!(rule_reason("node_modules", "x").starts_with("Dependency"));
        assert_eq!(rule_reason("mystery", "stored text"), "stored text");
        set(Lang::Tr);
        assert!(rule_reason("node_modules", "x").starts_with("Bağımlılık"));
        set(Lang::En);
    }

    #[test]
    fn every_rule_the_engine_emits_has_a_translation() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // keep in sync with rules.rs: a rule without an entry silently stays Turkish in English mode
        for rule in [
            "protected",
            "installed_app",
            "system_file",
            "recently_modified",
            "safe_ext",
            "temp_location",
            "vm_disk",
            "installer_image",
            "archive",
            "large_file",
            "node_modules",
            "rust_target",
            "native_build",
            "android_build",
            "gradle_build",
            "gradle_cache",
            "py_cache",
            "cache_dir",
        ] {
            assert_ne!(rule_reason(rule, "STORED"), "STORED", "{rule}");
        }
    }
}
