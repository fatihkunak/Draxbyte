//! Minimal prompt: file metadata and path only, never file contents.

use crate::models::{human_bytes, ScanItem};

pub const MAX_TOKENS: u64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

pub fn build(item: &ScanItem, now: i64) -> Prompt {
    let system = "You judge whether a file or folder on a user's disk can be deleted. \
Reply in at most 40 words, starting with one of SAFE, CAUTION or KEEP, then the reason."
        .to_string();
    let user = format!(
        "path: {}\nkind: {}\nsize: {}\nage_days: {}\nrule_hint: {}",
        item.path.display(),
        if item.is_dir { "folder" } else { "file" },
        human_bytes(item.size),
        item.age_days(now),
        item.rule
    );
    Prompt { system, user }
}

/// Rough budget used for the daily-limit check before a request (~4 chars per
/// token, plus room for the reply). The real count from the API replaces it afterwards.
pub fn estimate_tokens(p: &Prompt) -> u64 {
    (p.system.len() + p.user.len()) as u64 / 4 + MAX_TOKENS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Category;
    use std::path::PathBuf;

    fn item() -> ScanItem {
        ScanItem {
            path: PathBuf::from("/data/blob.bin"),
            size: 800 * 1024 * 1024,
            mtime: 0,
            is_dir: false,
            category: Category::Yellow,
            rule: "large_file".into(),
            reason: "r".into(),
            unknown: true,
        }
    }

    #[test]
    fn prompt_has_metadata_only_and_is_small() {
        let p = build(&item(), 90 * 86_400);
        assert!(p.user.contains("/data/blob.bin"));
        assert!(p.user.contains("800.0 MB"));
        assert!(p.user.contains("age_days: 90"));
        assert!(p.user.contains("large_file"));
        assert!(
            estimate_tokens(&p) < 300,
            "prompt should stay tiny, got {}",
            estimate_tokens(&p)
        );
    }
}
