//! Read-only Docker disk check: `docker system df`. Never prunes or touches containers.

use crate::i18n::t;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerRow {
    pub kind: String,
    pub total: usize,
    pub active: usize,
    pub size: String,
    pub reclaimable: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerStatus {
    /// `docker` is not on PATH.
    NotInstalled,
    /// CLI found but the daemon did not answer (Docker Desktop stopped?).
    DaemonDown(String),
    Ok(Vec<DockerRow>),
}

/// `docker` command with the platform quirks applied.
/// A GUI app started from Finder gets PATH=/usr/bin:/bin:/usr/sbin:/sbin, which has no Docker.
fn docker_command() -> Command {
    let mut c = Command::new("docker");
    #[cfg(target_os = "macos")]
    {
        let extra = "/usr/local/bin:/opt/homebrew/bin:/Applications/Docker.app/Contents/Resources/bin";
        let path = std::env::var("PATH").unwrap_or_default();
        c.env("PATH", format!("{extra}:{path}"));
    }
    c
}

fn command() -> Command {
    let mut c = docker_command();
    c.args(["system", "df", "--format", "{{json .}}"])
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from the GUI
    }
    c
}

/// Parse the one-JSON-object-per-line output of `docker system df --format '{{json .}}'`.
pub fn parse(stdout: &str) -> Vec<DockerRow> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .map(|v| {
            let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let n = |k: &str| s(k).parse().unwrap_or(0);
            DockerRow {
                kind: s("Type"),
                total: n("TotalCount"),
                active: n("Active"),
                size: s("Size"),
                reclaimable: s("Reclaimable"),
            }
        })
        .collect()
}

/// Runs `docker system df` with a timeout so a hung daemon cannot freeze the caller.
pub fn check() -> DockerStatus {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(command().output());
    });
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(out)) if out.status.success() => {
            DockerStatus::Ok(parse(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            DockerStatus::DaemonDown(err)
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => DockerStatus::NotInstalled,
        Ok(Err(e)) => DockerStatus::DaemonDown(e.to_string()),
        Err(_) => DockerStatus::DaemonDown(
            t("zaman aşımı (15 sn)", "timed out (15 s)").to_string(),
        ),
    }
}

/// Removes ONLY the Docker build cache (`docker builder prune -f`). Containers, images and
/// volumes are untouched. Returns Docker's summary line (e.g. "Total reclaimed space: 2GB").
pub fn prune_build_cache() -> Result<String, String> {
    let mut c = docker_command();
    c.args(["builder", "prune", "-f"]).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(c.output());
    });
    match rx.recv_timeout(Duration::from_secs(300)) {
        Ok(Ok(out)) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            Ok(s.lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string())
        }
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(t("zaman aşımı (5 dk)", "timed out (5 min)").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_system_df_lines() {
        let out = concat!(
            r#"{"Active":"17","Reclaimable":"0B (0%)","Size":"13.33GB","TotalCount":"17","Type":"Images"}"#,
            "\n",
            r#"{"Active":"0","Reclaimable":"2.005GB","Size":"2.719GB","TotalCount":"91","Type":"Build Cache"}"#,
            "\n"
        );
        let rows = parse(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "Images");
        assert_eq!(rows[0].total, 17);
        assert_eq!(rows[1].reclaimable, "2.005GB");
    }

    #[test]
    fn garbage_yields_no_rows() {
        assert!(parse("not json\n\n").is_empty());
    }
}
