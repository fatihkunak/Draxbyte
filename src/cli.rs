//! Headless mode: `draxbyte scan` / `draxbyte clean` (dry-run unless `--apply`).

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use draxbyte::config::Config;
use draxbyte::core::cleaner::{self, CleanOptions};
use draxbyte::core::scanner::{self, Progress, ScanOptions};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, Category, Outcome, ScanItem};
use draxbyte::tf;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Parser)]
#[command(
    name = "draxbyte",
    version,
    about = "Local-first disk scanner and cleaner"
)]
pub struct Cli {
    /// Alternative config.toml
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Scan and print what was found (changes nothing).
    Scan {
        /// Folders to scan (default: config scan.default_paths)
        paths: Vec<PathBuf>,
        /// Show at most this many rows
        #[arg(long, default_value_t = 40)]
        limit: usize,
        /// Store the scan so the GUI shows it as the last scan
        #[arg(long)]
        save: bool,
    },
    /// Clean what a scan finds. DRY-RUN unless --apply is given.
    /// Read-only Docker disk usage check (`docker system df`).
    Docker,
    Clean {
        paths: Vec<PathBuf>,
        /// Also include yellow ("be careful") items. Green only by default.
        #[arg(long)]
        yellow: bool,
        /// Really delete (to the Trash). Without it nothing is touched.
        #[arg(long)]
        apply: bool,
        /// Skip the Trash and delete for good (asks you to type SIL).
        #[arg(long, requires = "apply")]
        permanent: bool,
    },
}

fn run_scan(cfg: &Config, paths: Vec<PathBuf>) -> scanner::ScanOutput {
    let opts = ScanOptions::from_config(cfg, (!paths.is_empty()).then_some(paths));
    eprintln!(
        "{}",
        tf!(
            "Taranıyor: {}",
            "Scanning: {}",
            opts.roots
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    );
    let out = scanner::scan(&opts, &AtomicBool::new(false), &Progress::default());
    eprintln!(
        "{}",
        tf!(
            "{} öğe bulundu ({}), {} dosya gezildi, {} hata",
            "{} items found ({}), {} files scanned, {} errors",
            out.summary.item_count,
            human_bytes(out.summary.total_bytes),
            out.summary.files_visited,
            out.summary.errors
        )
    );
    out
}

pub fn run(cli: Cli, cfg: Config) -> Result<()> {
    match cli.cmd {
        None => unreachable!("GUI path is handled by main"),
        Some(Cmd::Scan { paths, limit, save }) => {
            let out = run_scan(&cfg, paths);
            if save {
                let mut db =
                    draxbyte::db::Db::open(&draxbyte::config::data_dir().join("draxbyte.db"))?;
                db.save_session(&out.summary, &out.items)?;
                eprintln!(
                    "{}",
                    t(
                        "Tarama kaydedildi (GUI'de son tarama olarak görünür)",
                        "Scan saved (shown as the last scan in the GUI)"
                    )
                );
            }
            let items = out.items;
            let now = draxbyte::models::now_secs();
            println!(
                "{:<8} {:>10} {:>6}  {:<16} {}",
                t("KATEGORİ", "CATEGORY"),
                t("BOYUT", "SIZE"),
                t("YAŞ(g)", "AGE(d)"),
                t("KURAL", "RULE"),
                t("YOL", "PATH")
            );
            for i in items.iter().take(limit) {
                println!(
                    "{:<8} {:>10} {:>6}  {:<16} {}",
                    i.category.label(),
                    human_bytes(i.size),
                    i.age_days(now),
                    i.rule,
                    i.path.display()
                );
            }
            if items.len() > limit {
                println!(
                    "{}",
                    tf!("... ve {} öğe daha", "... and {} more", items.len() - limit)
                );
            }
            Ok(())
        }
        Some(Cmd::Docker) => {
            use draxbyte::core::docker::{check, DockerStatus};
            match check() {
                DockerStatus::NotInstalled => {
                    bail!("{}", t("Docker bulunamadı", "Docker not found"))
                }
                DockerStatus::DaemonDown(e) => bail!(
                    "{}",
                    tf!("Docker yanıt vermiyor: {e}", "Docker is not responding: {e}")
                ),
                DockerStatus::Ok(rows) => {
                    println!(
                        "{:<14} {:>8} {:>8} {:>12} {}",
                        t("TÜR", "TYPE"),
                        t("ADET", "COUNT"),
                        t("AKTİF", "ACTIVE"),
                        t("BOYUT", "SIZE"),
                        t("GERİ KAZANILABİLİR", "RECLAIMABLE")
                    );
                    for r in rows {
                        println!(
                            "{:<14} {:>8} {:>8} {:>12} {}",
                            r.kind, r.total, r.active, r.size, r.reclaimable
                        );
                    }
                    Ok(())
                }
            }
        }
        Some(Cmd::Clean {
            paths,
            yellow,
            apply,
            permanent,
        }) => {
            let items: Vec<ScanItem> = run_scan(&cfg, paths)
                .items
                .into_iter()
                .filter(|i| {
                    i.category == Category::Green || (yellow && i.category == Category::Yellow)
                })
                .collect();
            let total: u64 = items.iter().map(|i| i.size).sum();
            println!(
                "{} {}",
                tf!(
                    "{} öğe, {}",
                    "{} items, {}",
                    items.len(),
                    human_bytes(total)
                ),
                if apply {
                    t("silinecek", "to be removed")
                } else {
                    t("(kuru çalıştırma)", "(dry run)")
                }
            );
            if permanent {
                print!(
                    "{}",
                    tf!(
                        "KALICI silme. Onaylamak için SIL yaz: ",
                        "PERMANENT delete. Type DELETE to confirm: "
                    )
                );
                std::io::stdout().flush()?;
                let mut s = String::new();
                std::io::stdin().read_line(&mut s)?;
                if s.trim() != draxbyte::i18n::confirm_word() {
                    bail!("{}", t("iptal edildi", "cancelled"));
                }
            }
            let report = cleaner::clean(
                &items,
                CleanOptions {
                    dry_run: !apply,
                    permanent,
                },
                &AtomicBool::new(false),
                |_, _| {},
            );
            for e in &report.entries {
                let tag = match &e.outcome {
                    Outcome::DryRun => t("silinirdi", "would delete").to_string(),
                    Outcome::Trashed => t("çöpe taşındı", "moved to Bin").to_string(),
                    Outcome::Deleted => t("silindi", "deleted").to_string(),
                    Outcome::Skipped(m) => tf!("atlandı ({m})", "skipped ({m})"),
                    Outcome::Failed(m) => tf!("HATA ({m})", "ERROR ({m})"),
                };
                println!(
                    "{:>10}  {:<24} {}",
                    human_bytes(e.size),
                    tag,
                    e.path.display()
                );
            }
            println!(
                "{}",
                tf!("Toplam: {}", "Total: {}", human_bytes(report.freed_bytes))
            );
            Ok(())
        }
    }
}
