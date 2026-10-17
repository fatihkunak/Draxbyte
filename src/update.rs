//! Update check: asks GitHub for the latest release tag. Check only — nothing is downloaded or
//! installed. Runs when the user presses the button, or at start-up if `ui.check_updates` is on.

use serde::Deserialize;
use std::time::Duration;

pub const API_URL: &str = "https://api.github.com/repos/fatihkunak/Draxbyte/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Raw {
    tag_name: String,
    html_url: String,
}

/// "v0.2.1" / "0.2.1" / "0.2" → (0, 2, 1). Pre-release suffixes ("-rc1") are ignored.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = it.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}

/// True when `latest` is strictly newer than `current`. Unparseable input is never "newer".
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

pub async fn latest() -> Result<Release, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("draxbyte/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let raw: Raw = resp.json().await.map_err(|e| e.to_string())?;
    Ok(Release {
        tag: raw.tag_name,
        url: raw.html_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags() {
        assert_eq!(parse_version("v0.2.1"), Some((0, 2, 1)));
        assert_eq!(parse_version("1.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn newer_is_strict_and_numeric() {
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("0.9.0", "v0.10.0")); // numeric, not lexical
        assert!(!is_newer("0.2.0", "v0.2.0"));
        assert!(!is_newer("0.3.0", "v0.2.9"));
        assert!(!is_newer("0.1.0", "garbage"));
    }
}

#[cfg(test)]
mod live {
    /// Real network call; run with `cargo test update::live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn latest_release_is_readable() {
        let r = super::latest().await.expect("GitHub reachable");
        assert!(super::parse_version(&r.tag).is_some(), "tag {}", r.tag);
        assert!(r.url.starts_with("https://github.com/"));
    }
}
