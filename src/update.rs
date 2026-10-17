//! Update check and download. Checking asks GitHub for the latest release tag; downloading
//! fetches `Draxbyte-Setup.exe`, verifies it (SHA256 from the same release + a pinned
//! code-signing certificate) and only then hands it to the caller to run. Both happen only on
//! an explicit user action (or the opt-in start-up check, which never downloads).

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

pub const API_URL: &str = "https://api.github.com/repos/fatihkunak/Draxbyte/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/fatihkunak/Draxbyte/releases/download/";
const SETUP_NAME: &str = "Draxbyte-Setup.exe";
const SUMS_NAME: &str = "SHA256SUMS.txt";
const MAX_SETUP_BYTES: u64 = 150 * 1024 * 1024;
/// Thumbprint of `assets/draxbyte-codesign.cer` ("CN=Fatih Kunak (Draxbyte)"). An installer
/// signed by anything else is refused, even if its hash matches.
pub const SIGNER_THUMBPRINT: &str = "C6291C28A5E34C7B10623D7876B277362CE320E0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub setup_url: Option<String>,
    pub sums_url: Option<String>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Raw {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

/// Bytes downloaded so far / expected total (0 until the server tells us).
#[derive(Default)]
pub struct DownloadProgress {
    pub done: AtomicU64,
    pub total: AtomicU64,
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

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!("draxbyte/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn latest() -> Result<Release, String> {
    let resp = client(Duration::from_secs(10))?
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let raw: Raw = resp.json().await.map_err(|e| e.to_string())?;
    let find = |name: &str| {
        raw.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Ok(Release {
        setup_url: find(SETUP_NAME),
        sums_url: find(SUMS_NAME),
        tag: raw.tag_name,
        url: raw.html_url,
    })
}

/// Finds `name`'s hash in `sha256sum`-style text ("<hex>  <name>", optional `*` before the name).
pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn pinned_url(url: &str) -> Result<&str, String> {
    if url.starts_with(DOWNLOAD_PREFIX) {
        Ok(url)
    } else {
        Err(format!("unexpected download address: {url}"))
    }
}

/// Downloads the installer of `rel` into the temp folder and verifies it. Returns its path.
/// Nothing is executed here.
pub async fn download_setup(rel: &Release, progress: &DownloadProgress) -> Result<PathBuf, String> {
    let setup_url = pinned_url(rel.setup_url.as_deref().ok_or("this release has no installer")?)?;
    let sums_url = pinned_url(rel.sums_url.as_deref().ok_or("this release has no checksums")?)?;
    let c = client(Duration::from_secs(600))?;

    let sums = c
        .get(sums_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let expected = parse_sums(&sums, SETUP_NAME).ok_or("checksum for the installer not found")?;

    let mut resp = c
        .get(setup_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    if total > MAX_SETUP_BYTES {
        return Err("installer is unexpectedly large".into());
    }
    progress.total.store(total, Relaxed);

    let dir = std::env::temp_dir().join(format!("draxbyte-update-{}", rel.tag.trim_start_matches('v')));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(SETUP_NAME);
    let result = write_and_hash(&mut resp, &path, progress).await;
    let actual = match result {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
    };
    if actual != expected {
        let _ = std::fs::remove_file(&path);
        return Err("checksum mismatch — the download is corrupt or was tampered with".into());
    }
    if let Err(e) = verify_signature(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(path)
}

async fn write_and_hash(
    resp: &mut reqwest::Response,
    path: &Path,
    progress: &DownloadProgress,
) -> Result<String, String> {
    use std::io::Write;
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        done += chunk.len() as u64;
        if done > MAX_SETUP_BYTES {
            return Err("installer is unexpectedly large".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        progress.done.store(done, Relaxed);
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(hex::encode(hasher.finalize()))
}

/// Interprets `"<Status>|<Thumbprint>"` as printed by `Get-AuthenticodeSignature`.
/// A self-signed certificate is reported as `UnknownError` (untrusted root) — that is expected;
/// what matters is that the file is signed, not altered (`HashMismatch`), and signed by OUR key.
pub fn check_signature_output(out: &str) -> Result<(), String> {
    let (status, thumb) = out.trim().split_once('|').ok_or("unreadable signature result")?;
    match status {
        "NotSigned" => return Err("the installer is not signed".into()),
        "HashMismatch" => return Err("the installer was modified after signing".into()),
        _ => {}
    }
    if thumb.trim().eq_ignore_ascii_case(SIGNER_THUMBPRINT) {
        Ok(())
    } else {
        Err("the installer is signed by an unknown publisher".into())
    }
}

#[cfg(windows)]
fn verify_signature(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // Path goes through the environment: `-Command` does not fill `$args` with trailing arguments.
    let script = "$s = Get-AuthenticodeSignature -LiteralPath $env:DRAXBYTE_VERIFY_PATH; \
                  \"$($s.Status)|$($s.SignerCertificate.Thumbprint)\"";
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DRAXBYTE_VERIFY_PATH", path)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    check_signature_output(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(windows))]
fn verify_signature(_path: &Path) -> Result<(), String> {
    Err("in-app update is only available on Windows".into())
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

    #[test]
    fn finds_hash_for_the_installer() {
        let h = "a".repeat(64);
        let text = format!("{}  draxbyte.exe\n{}  Draxbyte-Setup.exe\n", "b".repeat(64), h.to_uppercase());
        assert_eq!(parse_sums(&text, "Draxbyte-Setup.exe"), Some(h));
        assert_eq!(parse_sums(&text, "missing.exe"), None);
        assert_eq!(parse_sums("short  Draxbyte-Setup.exe", "Draxbyte-Setup.exe"), None);
    }

    #[test]
    fn only_project_release_urls_are_accepted() {
        assert!(pinned_url("https://github.com/fatihkunak/Draxbyte/releases/download/v0.3.0/Draxbyte-Setup.exe").is_ok());
        assert!(pinned_url("https://evil.example/Draxbyte-Setup.exe").is_err());
        assert!(pinned_url("https://github.com/someone/else/releases/download/v1/x.exe").is_err());
    }

    #[test]
    fn signature_rules() {
        let ok = format!("UnknownError|{}", SIGNER_THUMBPRINT.to_lowercase());
        assert!(check_signature_output(&ok).is_ok());
        assert!(check_signature_output(&format!("Valid|{SIGNER_THUMBPRINT}")).is_ok());
        assert!(check_signature_output("NotSigned|").is_err());
        assert!(check_signature_output(&format!("HashMismatch|{SIGNER_THUMBPRINT}")).is_err());
        assert!(check_signature_output("UnknownError|0000000000000000000000000000000000000000").is_err());
        assert!(check_signature_output("garbage").is_err());
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
        assert!(r.setup_url.is_some() && r.sums_url.is_some(), "release assets");
    }

    /// Downloads + verifies the real installer of the latest release (does not run it).
    #[tokio::test]
    #[ignore]
    async fn latest_installer_downloads_and_verifies() {
        let r = super::latest().await.expect("GitHub reachable");
        let p = super::DownloadProgress::default();
        let path = super::download_setup(&r, &p).await.expect("download + verify");
        assert!(path.exists());
        assert!(p.done.load(std::sync::atomic::Ordering::Relaxed) > 1_000_000);
    }
}
