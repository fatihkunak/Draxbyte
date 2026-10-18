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
        /// List the paths that could not be read, with the reason
        #[arg(long)]
        show_errors: bool,
    },
    /// Clean what a scan finds. DRY-RUN unless --apply is given.
    /// Read-only Docker disk usage check (`docker system df`).
    Docker,
    /// Full optimization: cleanup + duplicates + Docker cache + Recycle Bin + memory.
    /// Only measures and prints unless --apply is given.
    Optimize {
        /// Really do it (files go to the Recycle Bin, then the bin is emptied unless --keep-bin)
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        skip_cleanup: bool,
        #[arg(long)]
        skip_dupes: bool,
        #[arg(long)]
        skip_docker: bool,
        /// Docker parts to clean, comma separated: build, dangling, networks (default), stopped, images, volumes.
        /// `stopped` and `volumes` can lose data; they run only when named here.
        #[arg(long, value_delimiter = ',')]
        docker_parts: Vec<String>,
        /// Do not empty the Recycle Bin at the end
        #[arg(long)]
        keep_bin: bool,
        #[arg(long)]
        skip_memory: bool,
        /// Folders for the duplicate search (default: your home folder)
        #[arg(long = "dupe-path")]
        dupe_paths: Vec<PathBuf>,
        #[arg(long, default_value_t = 10)]
        dupe_min_mb: u64,
    },
    /// Show the Recycle Bin / Trash size; `--empty` empties it for good.
    Recycle {
        #[arg(long)]
        empty: bool,
    },
    /// Find identical files. Changes nothing unless --apply is given.
    Dupes {
        /// Folders to search (default: your home folder)
        paths: Vec<PathBuf>,
        /// Ignore files smaller than this
        #[arg(long, default_value_t = 1)]
        min_size_mb: u64,
        /// Show at most this many groups
        #[arg(long, default_value_t = 15)]
        limit: usize,
        /// Also look inside node_modules, .git, target, caches…
        #[arg(long)]
        include_dev: bool,
        /// Remove the extra copies (one per group always stays)
        #[arg(long)]
        apply: bool,
        /// Which copy stays
        #[arg(long, value_enum, default_value_t = KeepArg::Oldest)]
        keep: KeepArg,
        /// Skip the Recycle Bin / Trash (asks you to type the confirmation word)
        #[arg(long, requires = "apply")]
        permanent: bool,
    },
    /// Print the biggest folders and files under a path (a text disk map).
    Map {
        /// Folder or drive (default: your home folder)
        path: Option<PathBuf>,
        /// How many levels to print
        #[arg(long, default_value_t = 2)]
        depth: usize,
        /// Entries per level
        #[arg(long, default_value_t = 12)]
        limit: usize,
    },
    /// List installed programs with size and last use (Windows and macOS).
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

/// Plain-text part name for the CLI report.
pub fn optimize_title(t: draxbyte::core::optimize::Task) -> &'static str {
    crate::optimize::task_title(t)
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
        Some(Cmd::Optimize { apply, skip_cleanup, skip_dupes, skip_docker, docker_parts, keep_bin, skip_memory, dupe_paths, dupe_min_mb }) => {
            use draxbyte::core::optimize::{self as opt, OptProgress, Scope};
            let mut scope = Scope { cleanup: !skip_cleanup, dupes: !skip_dupes, docker: !skip_docker, recycle_bin: !keep_bin, memory: !skip_memory, dupe_min_mb, ..Scope::default() };
            if !dupe_paths.is_empty() {
                scope.dupe_roots = dupe_paths;
            }
            if !docker_parts.is_empty() {
                use draxbyte::core::docker::{Part, Parts};
                let mut parts = Parts::none();
                for name in &docker_parts {
                    let part = match name.trim() {
                        "build" => Part::BuildCache,
                        "dangling" => Part::DanglingImages,
                        "networks" => Part::Networks,
                        "stopped" => Part::StoppedContainers,
                        "images" => Part::UnusedImages,
                        "volumes" => Part::Volumes,
                        other => bail!("{}", tf!("bilinmeyen Docker parçası: {}", "unknown Docker part: {}", other)),
                    };
                    parts.set(part, true);
                }
                scope.docker_parts = parts;
            }
            if !scope.any() {
                bail!("{}", t("hiçbir işlem seçilmedi", "no task selected"));
            }
            eprintln!("{}", t("Analiz ediliyor (salt okuma)…", "Analyzing (read-only)…"));
            let (cancel, progress) = (AtomicBool::new(false), OptProgress::default());
            let an = opt::analyze(&cfg, &scope, &cancel, &progress);
            println!("{}", tf!("Güvenli temizlik: {} öğe · {}", "Safe cleanup: {} items · {}", an.cleanup.len(), human_bytes(an.cleanup_bytes)));
            println!("{}", tf!("Kopya dosyalar: {} dosya · {} ({} grup bulundu)", "Duplicates: {} files · {} ({} groups found)", an.dupe_delete.len(), human_bytes(an.dupe_bytes), an.dupe_groups.len()));
            if let Some(b) = an.bin {
                println!("{}", tf!("Çöp Kutusu: {} öğe · {}", "Recycle Bin: {} items · {}", b.items, human_bytes(b.bytes)));
            }
            if let Some(d) = &an.docker {
                println!("{}", tf!(
                    "Docker: {} durmuş konteyner · {} etiketsiz imaj · {} boşta birim · seçili parçalardan tahmini {}",
                    "Docker: {} stopped containers · {} untagged images · {} unused volumes · estimated {} from the chosen parts",
                    d.stopped.len(), d.dangling_images.len(), d.volumes.len(), human_bytes(opt::docker_estimate(d, scope.docker_parts))
                ));
                for r in &d.rows {
                    println!("  {:<14} {:>4} ({} {})  {:>10}  {} {}", r.kind, r.total, r.active, t("aktif", "active"), r.size, t("geri kazanılabilir", "reclaimable"), r.reclaimable);
                }
                for v in d.volumes.iter().take(8) {
                    println!("  {} {:>10}  {}  {}", t("boşta birim", "unused volume"), v.size, v.name, v.info);
                }
                if let Some(img) = d.disk_image {
                    println!("{}", tf!("  Docker disk imajı: {} (temizlikle kendiliğinden küçülmez)", "  Docker disk image: {} (does not shrink by itself)", human_bytes(img)));
                }
            }
            if an.scan_errors > 0 {
                println!("{}", tf!("{} yol okunamadı", "{} paths could not be read", an.scan_errors));
            }
            if !apply {
                println!("{}", t("KURU ÇALIŞTIRMA: hiçbir şey değişmedi. Uygulamak için --apply ekle.", "DRY RUN: nothing was changed. Add --apply to run it."));
                return Ok(());
            }
            let rep = opt::apply(&an, &scope, false, &cancel, &progress);
            for s in &rep.steps {
                println!("{}: {} · {} {}", optimize_title(s.task), match &s.status { opt::StepStatus::Done => "OK".to_string(), opt::StepStatus::Skipped(w) => format!("skipped ({w})"), opt::StepStatus::Failed(e) => format!("FAILED ({e})") }, human_bytes(s.freed), s.detail);
            }
            println!("{}", tf!("Toplam kaldırılan: {} · boş yer {} → {}", "Total removed: {} · free space {} → {}", human_bytes(rep.freed()), human_bytes(rep.free_before), human_bytes(rep.free_after)));
            Ok(())
        }
        Some(Cmd::Scan { paths, limit, save, show_errors }) => {
            let out = run_scan(&cfg, paths);
            print_scan_errors(&out.errors, show_errors);
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
        Some(Cmd::Dupes { paths, min_size_mb, limit, include_dev, apply, keep, permanent }) => {
            dupes_cmd(paths, min_size_mb, limit, include_dev, apply, keep, permanent)
        }
        Some(Cmd::Map { path, depth, limit }) => map_cmd(path, depth, limit),
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

fn apps_cmd(unused_days: Option<i64>, store: bool, hidden: bool, limit: usize) -> Result<()> {
    use draxbyte::core::apps::{self, human_days, Removability};
    if !apps::supported() {
        bail!("{}", t("Program yönetimi bu platformda yok", "Program management is not available on this platform"));
    }
    let now = draxbyte::models::now_secs();
    let mut list = apps::inventory(store);
    apps::fill_missing_sizes(&mut list, &AtomicBool::new(false));
    list.retain(|a| hidden || !a.system_component);
    if let Some(n) = unused_days {
        list.retain(|a| a.days_unused(now).map(|d| d >= n).unwrap_or(true));
    }
    list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    println!("{:>10}  {:<16} {:<44} {}", t("BOYUT", "SIZE"), t("SON KULLANIM", "LAST USED"), t("PROGRAM", "PROGRAM"), t("DURUM", "STATUS"));
    for a in list.iter().take(limit) {
        let used = a.days_unused(now).map(human_days).unwrap_or_else(|| "?".into());
        let status = match &a.removable {
            Removability::Normal => String::new(),
            Removability::Blocked(_) => t("üretici engelli → zorla", "vendor-blocked → force").to_string(),
            Removability::Sealed(_) => t("sistem korumalı", "system-protected").to_string(),
        };
        println!("{:>10}  {:<16} {:<44} {}", human_bytes(a.size_bytes), used, a.name.chars().take(44).collect::<String>(), status);
    }
    println!("{}", tf!("{} program (gösterilen: ilk {})", "{} programs (showing first {})", list.len(), limit.min(list.len())));
    Ok(())
}

fn uninstall_cmd(query: &str, apply: bool, force: bool, silent: bool, trash: bool, probable: bool) -> Result<()> {
    use draxbyte::core::apps::{self, Confidence, RemoveOptions, Removability};
    if !apps::supported() {
        bail!("{}", t("Program yönetimi bu platformda yok", "Program management is not available on this platform"));
    }
    let all = apps::inventory(true);
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

    println!("{} {} {}  ({})", app.name, app.version, app.publisher, app.install_location.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
    println!("{}", match &app.removable {
        Removability::Normal => t("kaldırıcı: var", "uninstaller: available").to_string(),
        Removability::Blocked(m) => format!("{}: {m}", t("kaldırıcı", "uninstaller")),
        Removability::Sealed(m) => format!("{}: {m}", t("KALDIRILAMAZ", "CANNOT BE REMOVED")),
    });
    if let Removability::Sealed(m) = &app.removable {
        bail!("{m}");
    }
    let running = app.install_location.as_deref().map(apps::processes_under).unwrap_or_default();
    for g in &running {
        println!("{}", tf!("çalışıyor: {} ({} işlem)", "running: {} ({} processes)", g.name, g.pids.len()));
    }
    let leftovers = apps::find_leftovers(&app, &others);
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
        let r = apps::close_running(&app);
        println!("{}", tf!("{} işlem kapatıldı", "{} processes closed", r.asked));
    }
    match apps::create_restore_point(&app.name) {
        Ok(()) => println!("{}", t("Geri yükleme noktası oluşturuldu", "Restore point created")),
        Err(e) => println!("{}", tf!("Geri yükleme noktası oluşturulamadı ({e}); devam ediliyor", "Restore point not created ({e}); continuing")),
    }
    if !force {
        let by_force_flag = matches!(app.removable, Removability::Blocked(_));
        match apps::run_uninstaller(&app, silent, by_force_flag) {
            Ok(code) => println!("{}", tf!("Kaldırıcı bitti (çıkış kodu {})", "Uninstaller finished (exit code {})", code)),
            Err(e) => println!("{}", tf!("Kaldırıcı çalışmadı: {e}", "Uninstaller did not run: {e}")),
        }
        // Many uninstallers hand over to a copy of themselves and return at once.
        for _ in 0..60 {
            if !apps::still_registered(&app) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
    let after = apps::find_leftovers(&app, &others);
    let chosen: Vec<_> = after.into_iter().filter(|l| probable || l.confidence == Confidence::High).collect();
    let rep = apps::remove_leftovers(&app, &chosen, &RemoveOptions { permanent: !trash, backup_dir: apps::backup_dir_for(&app) });
    for i in &rep.items {
        println!("  {}  {}{}", if i.ok { "OK  " } else { "FAIL" }, i.label, if i.message.is_empty() { String::new() } else { format!("  ({})", i.message) });
    }
    println!(
        "{}",
        tf!("Geri kazanılan: {} · yedek: {}", "Freed: {} · backup: {}", human_bytes(rep.freed), rep.backup_dir.display())
    );
    match apps::after_removal(&app) {
        Some(Ok(m)) => println!("{m}"),
        Some(Err(e)) => println!("{}", tf!("Ek adım yapılamadı ({e}) — yönetici olarak çalıştır", "Extra step failed ({e}) — run as administrator")),
        None => {}
    }
    Ok(())
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum KeepArg {
    Oldest,
    Newest,
    Shortest,
}

fn dupes_cmd(paths: Vec<PathBuf>, min_size_mb: u64, limit: usize, include_dev: bool, apply: bool, keep: KeepArg, permanent: bool) -> Result<()> {
    use draxbyte::core::dupes::{self, DupeOptions, DupeProgress, Keep};
    let roots = if paths.is_empty() { vec![draxbyte::config::home_dir()] } else { paths };
    eprintln!("{}", tf!("Aranıyor: {}", "Searching: {}", roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")));
    let opts = DupeOptions { roots, min_size: min_size_mb * 1024 * 1024, skip_dev: !include_dev };
    let groups = dupes::find(&opts, &AtomicBool::new(false), &DupeProgress::default());
    let wasted: u64 = groups.iter().map(|g| g.wasted()).sum();
    println!("{}", tf!("{} kopya grubu · boşa giden alan: {}", "{} groups of duplicates · wasted space: {}", groups.len(), human_bytes(wasted)));
    let rule = match keep {
        KeepArg::Oldest => Keep::Oldest,
        KeepArg::Newest => Keep::Newest,
        KeepArg::Shortest => Keep::ShortestPath,
    };
    for g in groups.iter().take(limit) {
        let k = dupes::keep_index(g, rule);
        println!("{}", tf!("{} kopya × {} (boşa: {})", "{} copies × {} (wasted: {})", g.files.len(), human_bytes(g.size), human_bytes(g.wasted())));
        for (i, f) in g.files.iter().enumerate() {
            println!("  {} {}", if i == k { t("KALIR  ", "KEEP   ") } else { t("silinecek", "delete  ") }, f.path.display());
        }
    }
    if groups.len() > limit {
        println!("{}", tf!("... ve {} grup daha", "... and {} more groups", groups.len() - limit));
    }
    if !apply {
        println!("{}", t("KURU ÇALIŞTIRMA: hiçbir şey silinmedi. Fazla kopyaları kaldırmak için --apply ekle.", "DRY RUN: nothing was deleted. Add --apply to remove the extra copies."));
        return Ok(());
    }
    let mut files = Vec::new();
    for g in &groups {
        let k = dupes::keep_index(g, rule);
        files.extend(dupes::to_delete(g, k).into_iter().cloned());
    }
    let refs: Vec<&dupes::DupeFile> = files.iter().collect();
    let items = dupes::to_scan_items(&refs);
    if permanent {
        print!("{}", tf!("KALICI silme ({} dosya). Onaylamak için {} yaz: ", "PERMANENT delete ({} files). Type {} to confirm: ", items.len(), draxbyte::i18n::confirm_word()));
        std::io::stdout().flush()?;
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        if s.trim() != draxbyte::i18n::confirm_word() {
            bail!("{}", t("iptal edildi", "cancelled"));
        }
    }
    let report = cleaner::clean(&items, CleanOptions { dry_run: false, permanent }, &AtomicBool::new(false), |_, _| {});
    let ok = report.entries.iter().filter(|e| matches!(e.outcome, Outcome::Trashed | Outcome::Deleted)).count();
    println!("{}", tf!("{} dosya kaldırıldı · {} geri kazanıldı ({} atlandı/başarısız)", "{} files removed · {} freed ({} skipped/failed)", ok, human_bytes(report.freed_bytes), report.entries.len() - ok));
    Ok(())
}

fn map_cmd(path: Option<PathBuf>, depth: usize, limit: usize) -> Result<()> {
    use draxbyte::core::diskmap::{self, MapOptions, MapProgress, Node};
    let root = path.unwrap_or_else(draxbyte::config::home_dir);
    if !root.is_dir() {
        bail!("{}", tf!("klasör bulunamadı: {}", "folder not found: {}", root.display()));
    }
    eprintln!("{}", tf!("Harita çıkarılıyor: {}", "Mapping: {}", root.display()));
    let tree = diskmap::build(&root, &MapOptions::default(), &AtomicBool::new(false), &MapProgress::default());
    fn print(n: &Node, level: usize, depth: usize, limit: usize, total: u64) {
        for c in n.children.iter().take(limit) {
            let pct = if total > 0 { c.size as f64 * 100.0 / total as f64 } else { 0.0 };
            println!("{}{:>10} {:>5.1}%  {}{}", "  ".repeat(level), human_bytes(c.size), pct, c.name, if c.is_dir && !c.other { std::path::MAIN_SEPARATOR.to_string() } else { String::new() });
            if level + 1 < depth && c.is_dir && !c.other {
                print(c, level + 1, depth, limit, total);
            }
        }
    }
    println!("{} · {}", tree.path.display(), tf!("{} · {} dosya", "{} · {} files", human_bytes(tree.size), tree.files));
    print(&tree, 0, depth.max(1), limit, tree.size);
    Ok(())
}

/// A scan's unreadable paths: a one-line summary with the way out, and the examples on request.
fn print_scan_errors(e: &draxbyte::core::scanner::ScanErrors, show: bool) {
    use draxbyte::core::scanner::ErrKind;
    if e.total() == 0 {
        return;
    }
    eprintln!(
        "{}",
        tf!(
            "{} yol okunamadı: {} izin yok, {} başka program kullanıyor, {} diğer.",
            "{} paths could not be read: {} access denied, {} in use by another program, {} other.",
            e.total(),
            e.permission,
            e.in_use,
            e.other
        )
    );
    if e.permission > 0 {
        let how = if cfg!(windows) {
            t("Hepsini görmek için Draxbyte'ı yönetici olarak çalıştır.", "Run Draxbyte as administrator to see all of them.")
        } else {
            t("Hepsini görmek için Draxbyte'a Tam Disk Erişimi ver (Sistem Ayarları → Gizlilik ve Güvenlik).", "Give Draxbyte Full Disk Access (System Settings → Privacy & Security) to see all of them.")
        };
        eprintln!("{how}");
    }
    if show {
        for s in &e.samples {
            let kind = match s.kind {
                ErrKind::Permission => t("izin yok", "denied"),
                ErrKind::InUse => t("kullanımda", "in use"),
                ErrKind::Other => t("diğer", "other"),
            };
            eprintln!("  [{kind}] {} — {}", s.path.display(), s.message);
        }
        if (e.samples.len() as u64) < e.total() {
            eprintln!("{}", tf!("  … ilk {} örnek gösterildi", "  … the first {} examples are shown", e.samples.len()));
        }
    }
}
