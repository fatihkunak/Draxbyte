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
    /// Show the Recycle Bin / Trash size; `--empty` empties it for good.
    Recycle {
        #[arg(long)]
        empty: bool,
    },
    /// List installed programs with size and last use (Windows).
    Apps {
        /// Only programs not used for at least this many days (programs with no record are listed too)
        #[arg(long)]
        unused_days: Option<i64>,
        /// Include Microsoft Store packages (slower)
        #[arg(long)]
        store: bool,
        /// Include system components hidden from "Apps & features"
        #[arg(long)]
        hidden: bool,
        #[arg(long, default_value_t = 60)]
        limit: usize,
    },
    /// Show what removing a program would touch; with --apply remove it completely (Windows).
    Uninstall {
        /// Name (or part of it) as listed by `draxbyte apps`
        query: String,
        /// Really do it. Without this nothing is changed.
        #[arg(long)]
        apply: bool,
        /// Skip the vendor's uninstaller and delete the program by force
        #[arg(long)]
        force: bool,
        /// Ask the vendor's uninstaller for silent mode when it has one
        #[arg(long)]
        silent: bool,
        /// Send leftover files to the Recycle Bin instead of deleting them for good
        #[arg(long)]
        trash: bool,
        /// Also remove leftovers marked "probable" (default: only exact matches)
        #[arg(long)]
        probable: bool,
    },
    /// Memory use per program (and `--trim` to move idle memory out of RAM, Windows).
    Memory {
        #[arg(long)]
        trim: bool,
        #[arg(long, default_value_t = 15)]
        limit: usize,
    },
    /// Programs that start with Windows. `--disable NAME` / `--enable NAME` switch one.
    Startup {
        #[arg(long)]
        disable: Option<String>,
        #[arg(long)]
        enable: Option<String>,
    },
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
        Some(Cmd::Apps { unused_days, store, hidden, limit }) => apps_cmd(unused_days, store, hidden, limit),
        Some(Cmd::Uninstall { query, apply, force, silent, trash, probable }) => {
            uninstall_cmd(&query, apply, force, silent, trash, probable)
        }
        Some(Cmd::Memory { trim, limit }) => memory_cmd(trim, limit),
        Some(Cmd::Startup { disable, enable }) => startup_cmd(disable, enable),
        Some(Cmd::Recycle { empty }) => {
            use draxbyte::core::recycle;
            let before = recycle::status().map_err(|e| anyhow::anyhow!(e))?;
            println!(
                "{}",
                tf!("{} öğe, {}", "{} items, {}", before.items, human_bytes(before.bytes))
            );
            if empty {
                recycle::empty().map_err(|e| anyhow::anyhow!(e))?;
                println!("{}", t("Boşaltıldı.", "Emptied."));
            } else if before.items > 0 {
                println!("{}", t("Boşaltmak için: draxbyte recycle --empty", "To empty it: draxbyte recycle --empty"));
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

// ---- programs / memory / startup ----------------------------------------------------------------

fn memory_cmd(trim: bool, limit: usize) -> Result<()> {
    use draxbyte::core::memory;
    let (info, groups) = memory::snapshot();
    println!(
        "{}",
        tf!(
            "RAM: {} toplam, {} kullanımda, {} boş (kullanılabilir)",
            "RAM: {} total, {} in use, {} available",
            human_bytes(info.total),
            human_bytes(info.used()),
            human_bytes(info.available)
        )
    );
    for g in groups.iter().take(limit) {
        println!(
            "{:>10}  {:>3} {:<28}{}",
            human_bytes(g.bytes),
            g.pids.len(),
            g.name,
            if g.protected { t("  (sistem — kapatılamaz)", "  (system — cannot be closed)") } else { "" }
        );
    }
    if trim {
        let r = memory::trim_working_sets().map_err(|e| anyhow::anyhow!(e))?;
        println!(
            "{}",
            tf!(
                "{} işlemin boşta belleği diske alındı ({} atlandı). Boş bellek: {} → {}. Not: bu geçicidir, uygulamalar ihtiyaç duydukça geri yükler.",
                "Idle memory of {} processes moved out of RAM ({} skipped). Available: {} → {}. Note: temporary — apps read it back when they need it.",
                r.trimmed,
                r.skipped,
                human_bytes(r.available_before),
                human_bytes(r.available_after)
            )
        );
    }
    Ok(())
}

fn startup_cmd(disable: Option<String>, enable: Option<String>) -> Result<()> {
    use draxbyte::core::startup;
    let entries = startup::list();
    for (name, on) in [(disable, false), (enable, true)].into_iter().filter_map(|(n, o)| n.map(|n| (n, o))) {
        let e = entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| anyhow::anyhow!(tf!("başlangıçta '{}' yok", "no startup entry '{}'", name)))?;
        startup::set_enabled(e, on).map_err(|m| anyhow::anyhow!(m))?;
        println!("{} {}", if on { t("açıldı:", "enabled:") } else { t("kapatıldı:", "disabled:") }, e.name);
    }
    for e in startup::list() {
        println!("{:<9} {:<32} {}", if e.enabled { t("açık", "on") } else { t("kapalı", "off") }, e.name, e.command);
    }
    Ok(())
}

#[cfg(not(windows))]
fn apps_cmd(_: Option<i64>, _: bool, _: bool, _: usize) -> Result<()> {
    bail!("{}", t("Program yönetimi şu an yalnızca Windows'ta", "Program management is Windows-only for now"))
}
#[cfg(not(windows))]
fn uninstall_cmd(_: &str, _: bool, _: bool, _: bool, _: bool, _: bool) -> Result<()> {
    bail!("{}", t("Program yönetimi şu an yalnızca Windows'ta", "Program management is Windows-only for now"))
}

#[cfg(windows)]
fn apps_cmd(unused_days: Option<i64>, store: bool, hidden: bool, limit: usize) -> Result<()> {
    use draxbyte::core::apps::{human_days, win};
    let now = draxbyte::models::now_secs();
    let mut apps = win::inventory(store);
    win::fill_missing_sizes(&mut apps, &AtomicBool::new(false));
    apps.retain(|a| hidden || !a.system_component);
    if let Some(n) = unused_days {
        apps.retain(|a| a.days_unused(now).map(|d| d >= n).unwrap_or(true));
    }
    apps.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    println!("{:>10}  {:<16} {:<44} {}", t("BOYUT", "SIZE"), t("SON KULLANIM", "LAST USED"), t("PROGRAM", "PROGRAM"), t("DURUM", "STATUS"));
    for a in apps.iter().take(limit) {
        let used = a.days_unused(now).map(human_days).unwrap_or_else(|| "?".into());
        let status = match &a.removable {
            draxbyte::core::apps::Removability::Normal => String::new(),
            draxbyte::core::apps::Removability::Blocked(_) => t("üretici engelli → zorla", "vendor-blocked → force").to_string(),
            draxbyte::core::apps::Removability::Sealed(_) => t("Windows korumalı", "Windows-protected").to_string(),
        };
        println!("{:>10}  {:<16} {:<44} {}", human_bytes(a.size_bytes), used, a.name.chars().take(44).collect::<String>(), status);
    }
    println!("{}", tf!("{} program (gösterilen: ilk {})", "{} programs (showing first {})", apps.len(), limit.min(apps.len())));
    Ok(())
}

#[cfg(windows)]
fn uninstall_cmd(query: &str, apply: bool, force: bool, silent: bool, trash: bool, probable: bool) -> Result<()> {
    use draxbyte::core::apps::{win, Confidence, Removability};
    let all = win::inventory(true);
    let q = query.to_lowercase();
    let exact: Vec<_> = all.iter().filter(|a| a.name.to_lowercase() == q || a.id.to_lowercase() == q).collect();
    let found: Vec<_> = if exact.len() == 1 { exact } else { all.iter().filter(|a| a.name.to_lowercase().contains(&q)).collect() };
    let app = match found.as_slice() {
        [] => bail!("{}", tf!("'{}' adında program yok", "no program matches '{}'", query)),
        [one] => (*one).clone(),
        many => {
            for a in many {
                eprintln!("  {} {}", a.name, a.version);
            }
            bail!("{}", t("Birden çok eşleşme; adı daha tam yaz", "Several matches; type the name more exactly"));
        }
    };
    let others: Vec<_> = all.iter().filter(|a| a.id != app.id).cloned().collect();
    let env = win::ScanEnv::system();

    println!("{} {} {}  ({})", app.name, app.version, app.publisher, app.install_location.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
    println!("{}", match &app.removable {
        Removability::Normal => t("kaldırıcı: var", "uninstaller: available").to_string(),
        Removability::Blocked(m) => format!("{}: {m}", t("kaldırıcı", "uninstaller")),
        Removability::Sealed(m) => format!("{}: {m}", t("KALDIRILAMAZ", "CANNOT BE REMOVED")),
    });
    if let Removability::Sealed(_) = app.removable {
        bail!("{}", t("Bu paket Windows'a ait; hiçbir uygulama kaldıramaz", "This package belongs to Windows; no program can remove it"));
    }
    let running = app.install_location.as_deref().map(draxbyte::core::apps::processes_under).unwrap_or_default();
    for g in &running {
        println!("{}", tf!("çalışıyor: {} ({} işlem)", "running: {} ({} processes)", g.name, g.pids.len()));
    }
    let leftovers = win::find_leftovers(&app, &others, &env);
    for l in &leftovers {
        println!(
            "  [{}] {:>10}  {}  — {}",
            if l.confidence == Confidence::High { "x" } else { "?" },
            human_bytes(l.size),
            l.label,
            l.why
        );
    }
    if !apply {
        println!("{}", t("KURU ÇALIŞTIRMA: hiçbir şey değişmedi. Gerçekten kaldırmak için --apply ekle.", "DRY RUN: nothing changed. Add --apply to really remove it."));
        return Ok(());
    }

    if !running.is_empty() {
        let r = win::close_running(&app);
        println!("{}", tf!("{} işlem kapatıldı", "{} processes closed", r.asked));
    }
    match win::create_restore_point(&app.name) {
        Ok(()) => println!("{}", t("Geri yükleme noktası oluşturuldu", "Restore point created")),
        Err(e) => println!("{}", tf!("Geri yükleme noktası oluşturulamadı ({e}); devam ediliyor", "Restore point not created ({e}); continuing")),
    }
    if !force {
        let by_force_flag = matches!(app.removable, Removability::Blocked(_));
        match win::run_uninstaller(&app, silent, by_force_flag) {
            Ok(code) => println!("{}", tf!("Kaldırıcı bitti (çıkış kodu {})", "Uninstaller finished (exit code {})", code)),
            Err(e) => println!("{}", tf!("Kaldırıcı çalışmadı: {e}", "Uninstaller did not run: {e}")),
        }
        // Many uninstallers hand over to a copy of themselves and return at once.
        for _ in 0..60 {
            if !win::still_registered(&app) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
    let after = win::find_leftovers(&app, &others, &env);
    let chosen: Vec<_> = after.into_iter().filter(|l| probable || l.confidence == Confidence::High).collect();
    let dir = draxbyte::config::data_dir()
        .join("uninstall-backups")
        .join(format!("{}-{}", draxbyte::models::now_secs(), app.name.chars().filter(|c| c.is_alphanumeric()).collect::<String>()));
    let rep = win::remove_leftovers(&app, &chosen, &win::RemoveOptions { permanent: !trash, backup_dir: dir }, &env);
    for i in &rep.items {
        println!("  {}  {}{}", if i.ok { "OK  " } else { "FAIL" }, i.label, if i.message.is_empty() { String::new() } else { format!("  ({})", i.message) });
    }
    println!(
        "{}",
        tf!("Geri kazanılan: {} · yedek: {}", "Freed: {} · backup: {}", human_bytes(rep.freed), rep.backup_dir.display())
    );
    if app.name == "Microsoft Edge" {
        match win::block_edge_reinstall() {
            Ok(()) => println!("{}", t("Edge'in Windows Update ile geri gelmesi engellendi", "Edge is blocked from coming back through Windows Update")),
            Err(e) => println!("{}", tf!("Edge geri gelme engeli konamadı ({e}) — yönetici olarak çalıştır", "Could not block Edge from coming back ({e}) — run as administrator")),
        }
    }
    Ok(())
}
