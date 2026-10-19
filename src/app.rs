//! Application state and the glue between the UI and the background jobs.
//! Scanning and cleaning run on plain threads; AI requests run on the tokio runtime.

use crate::ui;
use draxbyte::ai::client::{AiAnswer, AiService};
use draxbyte::config::{data_dir, expand_path, Config};
use draxbyte::core::cleaner::{self, CleanOptions};
use draxbyte::core::docker::{self, DockerStatus};
use draxbyte::core::scanner::{self, Progress, ScanOptions, ScanOutput};
use draxbyte::db::Db;
use draxbyte::i18n::t;
use draxbyte::models::{Category, CleanReport, Outcome, ScanItem, ScanSummary};
use draxbyte::tf;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Optimize,
    Advisor,
    Scan,
    Results,
    Clean,
    Programs,
    System,
    DiskMap,
    Duplicates,
    Cache,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Size,
    Age,
    Category,
    Path,
}

pub struct AiNote {
    pub text: String,
    pub cached: bool,
}

pub struct Row {
    pub item: ScanItem,
    pub selected: bool,
    pub ai: Option<AiNote>,
}

pub struct DiskInfo {
    pub name: String,
    pub total: u64,
    pub free: u64,
}

pub struct ScanJob {
    pub progress: Arc<Progress>,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<ScanOutput>,
    pub started: Instant,
}

pub struct CleanJob {
    pub cancel: Arc<AtomicBool>,
    pub done: Arc<AtomicUsize>,
    pub total: usize,
    pub rx: Receiver<CleanReport>,
}

/// What the confirmation modal is currently asking.
pub struct Confirm {
    pub dry_run: bool,
    pub permanent: bool,
    pub typed: String,
}

/// A running installer download (see `draxbyte::update::download_setup`).
pub struct UpdateDl {
    pub progress: Arc<draxbyte::update::DownloadProgress>,
    pub rx: Receiver<Result<PathBuf, String>>,
}

pub enum UpdateState {
    UpToDate,
    Available(draxbyte::update::Release),
    Failed(String),
}

/// Recycle Bin card on the home page.
#[derive(Default)]
pub struct BinCard {
    pub status: Option<draxbyte::core::recycle::BinStatus>,
    pub job: Option<Receiver<Result<draxbyte::core::recycle::BinStatus, String>>>,
    pub confirm: bool,
    pub error: Option<String>,
    /// Bytes freed by the last "empty", for the confirmation line.
    pub freed: Option<u64>,
}

/// Result of a per-provider action in the Settings chain.
pub enum ChainOut {
    Models(Vec<String>),
    Test(String),
}

pub struct App {
    pub cfg: Config,
    pub cfg_draft: Config,
    pub cfg_path: PathBuf,
    pub db: Option<Db>,
    pub rt: tokio::runtime::Runtime,
    pub view: View,
    pub disk: Option<DiskInfo>,
    pub scan_paths_text: String,

    pub last: Option<ScanSummary>,
    /// What the last scan could not read (and why), shown with the way to fix it.
    pub last_errors: Option<draxbyte::core::scanner::ScanErrors>,
    pub scan: Option<ScanJob>,
    pub rows: Vec<Row>,
    pub visible: Vec<usize>,
    pub view_dirty: bool,
    pub filter_cat: Option<Category>,
    pub filter_text: String,
    pub sort: (SortKey, bool),

    pub confirm: Option<Confirm>,
    pub clean_job: Option<CleanJob>,
    pub report: Option<CleanReport>,
    pub confirm_clear_cache: bool,

    pub ai: Option<Arc<AiService>>,
    pub advisor: Option<Arc<draxbyte::ai::advisor::Advisor>>,
    pub adv: crate::advisor::AdvisorState,
    pub ai_error: Option<String>,
    pub ai_confirm: Option<usize>,
    pub ai_inflight: HashSet<PathBuf>,
    pub ai_tx: Sender<(PathBuf, Result<AiAnswer, String>)>,
    pub ai_rx: Receiver<(PathBuf, Result<AiAnswer, String>)>,

    pub models: Vec<String>,
    pub models_for: String,
    pub models_job: Option<Receiver<Result<Vec<String>, String>>>,
    pub test_job: Option<Receiver<Result<String, String>>>,
    pub test_status: Option<(bool, String)>,
    pub model_filter: String,
    /// Settings → AI chain: per entry (by position) the fetched models, the last result and the running jobs.
    pub chain_models: std::collections::HashMap<usize, Vec<String>>,
    pub chain_status: std::collections::HashMap<usize, (bool, String)>,
    pub chain_jobs: Vec<(usize, Receiver<Result<ChainOut, String>>)>,

    pub bin: BinCard,
    pub opt: crate::optimize::OptState,
    pub map: crate::map_dupes::DiskMapState,
    pub dupes: crate::map_dupes::DupesState,
    pub apps: crate::panels::AppsState,
    pub sys: crate::panels::SysState,
    pub docker: Option<DockerStatus>,
    pub docker_job: Option<Receiver<DockerStatus>>,
    pub update: Option<UpdateState>,
    pub update_job: Option<Receiver<Result<draxbyte::update::Release, String>>>,
    pub update_confirm: bool,
    pub update_dl: Option<UpdateDl>,
    pub update_dl_error: Option<String>,
    /// Verified installer waiting to be started (the app then closes so it can be replaced).
    pub update_ready: Option<PathBuf>,
    pub docker_confirm_prune: bool,
    pub docker_prune_job: Option<Receiver<Result<String, String>>>,

    pub toast: Option<(String, Instant)>,
    pub logo: egui::TextureHandle,
}

pub fn detect_disk() -> Option<DiskInfo> {
    let home = draxbyte::config::home_dir();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| home.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| DiskInfo {
            name: d.mount_point().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
        })
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config, cfg_path: PathBuf) -> Self {
        ui::apply_theme(&cc.egui_ctx);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");
        let mut db = Db::open(&data_dir().join("draxbyte.db")).ok();
        let (last, rows) = match db
            .as_mut()
            .and_then(|d| d.last_summary().ok().flatten().map(|s| (d, s)))
        {
            Some((d, s)) => {
                let items = d.load_items(s.session_id).unwrap_or_default();
                (
                    Some(s),
                    items
                        .into_iter()
                        .map(|item| Row {
                            item,
                            selected: false,
                            ai: None,
                        })
                        .collect(),
                )
            }
            None => (None, Vec::new()),
        };
        let (ai_tx, ai_rx) = channel();
        let scan_paths_text = cfg.scan.default_paths.join("; ");
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([256, 256], ui::ICON_RGBA),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            logo,
            cfg_draft: cfg.clone(),
            cfg,
            cfg_path,
            db,
            rt,
            // DRAXBYTE_VIEW=programs|system|... opens that screen first (screenshots, demos).
            view: match std::env::var("DRAXBYTE_VIEW").unwrap_or_default().as_str() {
                "programs" => View::Programs,
                "system" => View::System,
                "optimize" => View::Optimize,
                "advisor" => View::Advisor,
                "map" => View::DiskMap,
                "dupes" => View::Duplicates,
                "settings" => View::Settings,
                _ => View::Home,
            },
            disk: detect_disk(),
            scan_paths_text,
            last,
            last_errors: None,
            scan: None,
            rows,
            visible: Vec::new(),
            view_dirty: true,
            filter_cat: None,
            filter_text: String::new(),
            sort: (SortKey::Size, false),
            confirm: None,
            clean_job: None,
            report: None,
            confirm_clear_cache: false,
            ai: None,
            advisor: None,
            adv: Default::default(),
            ai_error: None,
            ai_confirm: None,
            ai_inflight: HashSet::new(),
            ai_tx,
            ai_rx,
            models: Vec::new(),
            models_for: String::new(),
            models_job: None,
            test_job: None,
            test_status: None,
            model_filter: String::new(),
            chain_models: Default::default(),
            chain_status: Default::default(),
            chain_jobs: Vec::new(),
            bin: BinCard::default(),
            opt: Default::default(),
            map: Default::default(),
            dupes: Default::default(),
            apps: Default::default(),
            sys: Default::default(),
            docker: None,
            docker_job: None,
            update: None,
            update_job: None,
            update_confirm: false,
            update_dl: None,
            update_dl_error: None,
            update_ready: None,
            docker_confirm_prune: false,
            docker_prune_job: None,
            toast: None,
        };
        // DRAXBYTE_OPT_DEMO=1: Docker + Recycle Bin analysis starts at once (screenshots, demos)
        if std::env::var("DRAXBYTE_OPT_DEMO").is_ok() {
            app.opt.scope.cleanup = false;
            app.opt.scope.dupes = false;
            app.opt.scope.memory = false;
            app.opt_analyze(&cc.egui_ctx);
        }
        app.rebuild_ai();
        // DRAXBYTE_ADVISOR_DEMO=<question>: opens the advisor and sends that question (demos, tests)
        if let Ok(q) = std::env::var("DRAXBYTE_ADVISOR_DEMO") {
            if app.advisor.is_some() {
                app.adv.consent = true;
                app.view = View::Advisor;
                app.advisor_send(&cc.egui_ctx, &q);
            }
        }
        if app.cfg.ui.check_updates {
            app.start_update_check(&cc.egui_ctx);
        }
        app
    }

    /// Downloads and verifies the installer of the available release. Caller must have confirmed.
    pub fn start_update_download(&mut self, ctx: &egui::Context) {
        let Some(UpdateState::Available(rel)) = &self.update else {
            return;
        };
        if self.update_dl.is_some() {
            return;
        }
        let rel = rel.clone();
        let progress = Arc::new(draxbyte::update::DownloadProgress::default());
        let (tx, rx) = channel();
        let (p, ctx) = (progress.clone(), ctx.clone());
        self.rt.spawn(async move {
            let _ = tx.send(draxbyte::update::download_setup(&rel, &p).await);
            ctx.request_repaint();
        });
        self.update_dl_error = None;
        self.update_dl = Some(UpdateDl { progress, rx });
    }

    /// Starts the verified installer and closes the app so the installer can overwrite the exe.
    pub fn launch_update(&mut self, ctx: &egui::Context) {
        let Some(path) = self.update_ready.take() else {
            return;
        };
        match std::process::Command::new(&path).spawn() {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.update_dl_error = Some(e.to_string()),
        }
    }

    /// One GET to GitHub's "latest release" endpoint; `poll_jobs` picks up the answer.
    pub fn start_update_check(&mut self, ctx: &egui::Context) {
        if self.update_job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let _ = tx.send(draxbyte::update::latest().await);
            ctx.request_repaint();
        });
        self.update_job = Some(rx);
    }

    /// Runs the read-only Docker check on a plain thread; `poll_jobs` picks up the result.
    pub fn start_docker_check(&mut self, ctx: &egui::Context) {
        if self.docker_job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(docker::check());
            ctx.request_repaint();
        });
        self.docker_job = Some(rx);
    }

    /// Reads the Recycle Bin size; with `empty` it empties the bin first. Caller must have confirmed.
    pub fn start_bin(&mut self, ctx: &egui::Context, empty: bool) {
        if self.bin.job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            use draxbyte::core::recycle;
            let res = if empty { recycle::empty().and_then(|_| recycle::status()) } else { recycle::status() };
            let _ = tx.send(res);
            ctx.request_repaint();
        });
        self.bin.job = Some(rx);
    }

    /// Prunes only the build cache, then re-checks usage. Caller must have confirmed.
    pub fn start_docker_prune(&mut self, ctx: &egui::Context) {
        if self.docker_prune_job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(docker::prune_build_cache());
            ctx.request_repaint();
        });
        self.docker_prune_job = Some(rx);
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// (Re)creates the AI service from the current config. Nothing network-related
    /// happens here; when AI is off the service simply does not exist.
    pub fn rebuild_ai(&mut self) {
        self.ai = None;
        self.advisor = None;
        self.ai_error = None;
        if !self.cfg.ai.enabled {
            return;
        }
        match AiService::new(&self.cfg.ai) {
            Ok(s) => self.ai = Some(Arc::new(s)),
            Err(e) => self.ai_error = Some(e.to_string()),
        }
        match draxbyte::ai::advisor::Advisor::new(&self.cfg.ai) {
            Ok(a) => self.advisor = Some(Arc::new(a)),
            Err(e) => {
                self.ai_error.get_or_insert(e.to_string());
            }
        }
    }

    // ---- scan -------------------------------------------------------------

    pub fn parsed_scan_paths(&self) -> Vec<PathBuf> {
        self.scan_paths_text
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(expand_path)
            .collect()
    }

    pub fn start_scan(&mut self, ctx: &egui::Context) {
        if self.scan.is_some() || self.clean_job.is_some() {
            return;
        }
        let paths = self.parsed_scan_paths();
        if paths.is_empty() {
            self.toast(t("Taranacak yol yok", "No path to scan"));
            return;
        }
        // Untouched field = "scan everything configured" (system folders, all drives); an edited
        // field is taken literally.
        let untouched = self.scan_paths_text == self.cfg.scan.default_paths.join("; ");
        let opts = ScanOptions::from_config(&self.cfg, (!untouched).then_some(paths));
        let progress = Arc::new(Progress::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        {
            let (progress, cancel, ctx) = (progress.clone(), cancel.clone(), ctx.clone());
            std::thread::spawn(move || {
                let out = scanner::scan(&opts, &cancel, &progress);
                let _ = tx.send(out);
                ctx.request_repaint();
            });
        }
        self.scan = Some(ScanJob {
            progress,
            cancel,
            rx,
            started: Instant::now(),
        });
        self.view = View::Scan;
    }

    pub fn cancel_scan(&self) {
        if let Some(j) = &self.scan {
            j.cancel.store(true, Relaxed);
        }
    }

    // ---- selection / view -------------------------------------------------

    pub fn refresh_view(&mut self) {
        let text = self.filter_text.to_lowercase();
        let mut v: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                let r = &self.rows[i];
                self.filter_cat.is_none_or(|c| r.item.category == c)
                    && (text.is_empty()
                        || r.item.path.to_string_lossy().to_lowercase().contains(&text))
            })
            .collect();
        let (key, asc) = self.sort;
        v.sort_by(|&a, &b| {
            let (x, y) = (&self.rows[a].item, &self.rows[b].item);
            let o = match key {
                SortKey::Size => x.size.cmp(&y.size),
                SortKey::Age => y.mtime.cmp(&x.mtime), // older (smaller mtime) = larger age
                SortKey::Category => x.category.cmp(&y.category),
                SortKey::Path => x.path.cmp(&y.path),
            };
            if asc {
                o
            } else {
                o.reverse()
            }
        });
        self.visible = v;
        self.view_dirty = false;
    }

    pub fn select_category(&mut self, cat: Category) {
        for r in &mut self.rows {
            r.selected = r.item.category == cat;
        }
    }

    pub fn selected(&self) -> impl Iterator<Item = &Row> {
        self.rows
            .iter()
            .filter(|r| r.selected && r.item.category != Category::Red)
    }

    // ---- clean ------------------------------------------------------------

    pub fn start_clean(&mut self, ctx: &egui::Context, opts: CleanOptions) {
        if self.clean_job.is_some() || self.scan.is_some() {
            return;
        }
        let items: Vec<ScanItem> = self.selected().map(|r| r.item.clone()).collect();
        if items.is_empty() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let total = items.len();
        {
            let (cancel, done, ctx) = (cancel.clone(), done.clone(), ctx.clone());
            std::thread::spawn(move || {
                let rep = cleaner::clean(&items, opts, &cancel, |d, _| {
                    done.store(d, Relaxed);
                    ctx.request_repaint();
                });
                let _ = tx.send(rep);
                ctx.request_repaint();
            });
        }
        self.clean_job = Some(CleanJob {
            cancel,
            done,
            total,
            rx,
        });
        self.report = None;
        self.view = View::Clean;
    }

    pub fn undo_last(&mut self) {
        let Some(rep) = &self.report else { return };
        match cleaner::undo(rep) {
            Ok(0) => self.toast(t(
                "Geri alınacak öğe bulunamadı (Çöp Kutusu boşaltılmış olabilir)",
                "Nothing to restore (the Recycle Bin may have been emptied)",
            )),
            Ok(n) => {
                self.toast(tf!(
                    "{n} öğe Çöp Kutusu'ndan geri getirildi",
                    "{n} item(s) restored from the Recycle Bin"
                ));
                if let Some(rep) = &mut self.report {
                    for e in rep
                        .entries
                        .iter_mut()
                        .filter(|e| e.outcome == Outcome::Trashed)
                    {
                        e.outcome = Outcome::Skipped(t("geri alındı", "restored").into());
                    }
                    rep.freed_bytes = 0;
                }
            }
            Err(e) => self.toast(tf!("Geri alma başarısız: {e}", "Undo failed: {e}")),
        }
    }

    // ---- AI ---------------------------------------------------------------

    /// Called only after the user approved (or when the answer is already cached and free).
    pub fn ask_ai(&mut self, ctx: &egui::Context, row_idx: usize) {
        let Some(svc) = self.ai.clone() else { return };
        let Some(row) = self.rows.get(row_idx) else {
            return;
        };
        let item = row.item.clone();
        if !self.ai_inflight.insert(item.path.clone()) {
            return;
        }
        let (tx, ctx) = (self.ai_tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let res = svc.ask(&item).await.map_err(|e| e.to_string());
            let _ = tx.send((item.path.clone(), res));
            ctx.request_repaint();
        });
    }

    // ---- provider models / connection test (user-initiated, from Settings) ----

    /// Identifies provider + address so a fetched model list is dropped when either changes.
    pub fn provider_key(&self) -> String {
        format!(
            "{}|{}",
            self.cfg_draft.ai.provider,
            self.cfg_draft.ai.base_url.trim()
        )
    }

    pub fn fetch_models(&mut self, ctx: &egui::Context) {
        let creds = match draxbyte::ai::providers::resolve(&self.cfg_draft.ai) {
            Ok(c) => c,
            Err(e) => {
                self.test_status = Some((false, e.to_string()));
                return;
            }
        };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let _ = tx.send(
                draxbyte::ai::providers::list_models(&creds)
                    .await
                    .map_err(|e| e.to_string()),
            );
            ctx.request_repaint();
        });
        self.models_job = Some(rx);
        self.models_for = self.provider_key();
        self.test_status = None;
    }

    /// Credentials of one chain entry as typed in Settings (its own key only, like the advisor does).
    fn chain_creds(&self, idx: usize) -> Result<draxbyte::ai::providers::Creds, String> {
        let ai = &self.cfg_draft.ai;
        let e = ai.chain.get(idx).ok_or_else(|| "?".to_string())?;
        let view = draxbyte::config::AiConfig { provider: e.provider.clone(), base_url: e.base_url.clone(), model: e.model.clone(), ..Default::default() };
        draxbyte::ai::providers::resolve_with_key(&view, ai.chain_key(e)).map_err(|e| e.to_string())
    }

    /// A chain entry's model list (`fetch`) or a tiny real request (`!fetch`), started by the user's click.
    pub fn chain_job(&mut self, idx: usize, fetch: bool, ctx: &egui::Context) {
        if self.chain_jobs.iter().any(|(i, _)| *i == idx) {
            return;
        }
        let creds = match self.chain_creds(idx) {
            Ok(c) => c,
            Err(e) => {
                self.chain_status.insert(idx, (false, e));
                return;
            }
        };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let r = if fetch {
                draxbyte::ai::providers::list_models(&creds).await.map(ChainOut::Models)
            } else {
                draxbyte::ai::providers::check(&creds).await.map(|(ms, tok)| {
                    ChainOut::Test(tf!("Bağlantı tamam · {ms} ms · {tok} token", "Connection OK · {ms} ms · {tok} tokens"))
                })
            };
            let _ = tx.send(r.map_err(|e| e.to_string()));
            ctx.request_repaint();
        });
        self.chain_jobs.push((idx, rx));
        self.chain_status.remove(&idx);
    }

    /// Entries moved or removed: positions no longer match the old results.
    pub fn chain_forget(&mut self) {
        self.chain_models.clear();
        self.chain_status.clear();
        self.chain_jobs.clear();
    }

    pub fn test_connection(&mut self, ctx: &egui::Context) {
        let creds = match draxbyte::ai::providers::resolve(&self.cfg_draft.ai) {
            Ok(c) => c,
            Err(e) => {
                self.test_status = Some((false, e.to_string()));
                return;
            }
        };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let r = draxbyte::ai::providers::check(&creds)
                .await
                .map(|(ms, tok)| {
                    tf!(
                        "Bağlantı tamam · {ms} ms · {tok} token",
                        "Connection OK · {ms} ms · {tok} tokens"
                    )
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.test_job = Some(rx);
        self.test_status = None;
    }

    // ---- polling ----------------------------------------------------------

    pub fn poll_jobs(&mut self) {
        self.panels_poll();
        if let Some(job) = &self.scan {
            if let Ok(out) = job.rx.try_recv() {
                self.scan = None;
                self.finish_scan(out);
            }
        }
        if let Some(job) = &self.clean_job {
            if let Ok(rep) = job.rx.try_recv() {
                self.clean_job = None;
                if let Some(db) = &mut self.db {
                    let _ = db.log_clean(&rep);
                }
                let gone: HashSet<&PathBuf> = rep
                    .entries
                    .iter()
                    .filter(|e| matches!(e.outcome, Outcome::Trashed | Outcome::Deleted))
                    .map(|e| &e.path)
                    .collect();
                self.rows.retain(|r| !gone.contains(&r.item.path));
                self.view_dirty = true;
                self.report = Some(rep);
            }
        }
        if let Some(rx) = &self.bin.job {
            if let Ok(res) = rx.try_recv() {
                self.bin.job = None;
                match res {
                    Ok(st) => {
                        if let Some(before) = self.bin.status.filter(|_| self.bin.freed == Some(u64::MAX)) {
                            self.bin.freed = Some(before.bytes.saturating_sub(st.bytes));
                        }
                        self.bin.status = Some(st);
                        self.bin.error = None;
                    }
                    Err(e) => self.bin.error = Some(e),
                }
            }
        }
        if let Some(rx) = &self.docker_job {
            if let Ok(st) = rx.try_recv() {
                self.docker_job = None;
                self.docker = Some(st);
            }
        }
        if let Some(rx) = &self.update_job {
            if let Ok(res) = rx.try_recv() {
                self.update_job = None;
                self.update = Some(match res {
                    Ok(rel) if draxbyte::update::is_newer(env!("CARGO_PKG_VERSION"), &rel.tag) => {
                        UpdateState::Available(rel)
                    }
                    Ok(_) => UpdateState::UpToDate,
                    Err(e) => UpdateState::Failed(e),
                });
            }
        }
        if let Some(dl) = &self.update_dl {
            if let Ok(res) = dl.rx.try_recv() {
                self.update_dl = None;
                match res {
                    Ok(path) => self.update_ready = Some(path),
                    Err(e) => self.update_dl_error = Some(e),
                }
            }
        }
        if let Some(rx) = &self.docker_prune_job {
            if let Ok(res) = rx.try_recv() {
                self.docker_prune_job = None;
                match res {
                    Ok(summary) => {
                        self.toast(tf!(
                            "Build cache temizlendi. {summary}",
                            "Build cache cleaned. {summary}"
                        ));
                        self.docker = None; // stale numbers; the user re-checks
                    }
                    Err(e) => self.toast(tf!("Docker hatası: {e}", "Docker error: {e}")),
                }
            }
        }
        if let Some(rx) = &self.models_job {
            if let Ok(res) = rx.try_recv() {
                self.models_job = None;
                match res {
                    Ok(list) => {
                        self.test_status =
                            Some((true, tf!("{} model bulundu", "{} models found", list.len())));
                        self.models = list;
                    }
                    Err(e) => self.test_status = Some((false, e)),
                }
            }
        }
        let mut done: Vec<(usize, Result<ChainOut, String>)> = Vec::new();
        self.chain_jobs.retain(|(i, rx)| match rx.try_recv() {
            Ok(r) => {
                done.push((*i, r));
                false
            }
            Err(_) => true,
        });
        for (i, r) in done {
            match r {
                Ok(ChainOut::Models(list)) => {
                    self.chain_status.insert(i, (true, tf!("{} model bulundu", "{} models found", list.len())));
                    self.chain_models.insert(i, list);
                }
                Ok(ChainOut::Test(m)) => {
                    self.chain_status.insert(i, (true, m));
                }
                Err(e) => {
                    self.chain_status.insert(i, (false, e));
                }
            }
        }
        if let Some(rx) = &self.test_job {
            if let Ok(res) = rx.try_recv() {
                self.test_job = None;
                self.test_status = Some(match res {
                    Ok(m) => (true, m),
                    Err(e) => (false, e),
                });
            }
        }
        while let Ok((path, res)) = self.ai_rx.try_recv() {
            self.ai_inflight.remove(&path);
            match res {
                Ok(a) => {
                    if let Some(r) = self.rows.iter_mut().find(|r| r.item.path == path) {
                        r.ai = Some(AiNote {
                            text: a.text,
                            cached: a.cached,
                        });
                    }
                }
                Err(e) => self.toast(format!("AI: {e}")),
            }
        }
        if let Some((_, at)) = &self.toast {
            if at.elapsed().as_secs() >= 6 {
                self.toast = None;
            }
        }
    }

    fn finish_scan(&mut self, out: ScanOutput) {
        self.last_errors = Some(out.errors).filter(|e| e.total() > 0);
        let mut summary = out.summary;
        if !out.cancelled {
            if let Some(db) = &mut self.db {
                match db.save_session(&summary, &out.items) {
                    Ok(id) => summary.session_id = id,
                    Err(e) => tracing::warn!("scan could not be saved: {e}"),
                }
            }
        }
        self.rows = out
            .items
            .into_iter()
            .map(|item| Row {
                item,
                selected: false,
                ai: None,
            })
            .collect();
        self.view_dirty = true;
        self.filter_cat = None;
        self.last = Some(summary);
        self.disk = detect_disk();
        self.view = View::Results;
        if out.cancelled {
            self.toast(t(
                "Tarama iptal edildi (kısmi sonuçlar gösteriliyor, kaydedilmedi)",
                "Scan cancelled (showing partial results, not saved)",
            ));
        }
    }
}
