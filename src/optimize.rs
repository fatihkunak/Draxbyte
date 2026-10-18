//! State and background jobs of the Full optimization wizard (the screen is in
//! `ui/optimize_view.rs`; the work is in `core::optimize`). Analysis is read-only; `opt_apply`
//! only runs after the user approved the review.

use crate::app::App;
use draxbyte::core::optimize::{self, Analysis, OptProgress, RunReport, Scope, StepStatus, Task};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, Outcome};
use draxbyte::tf;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OptStep {
    Scope,
    Analyzing,
    Review,
    Running,
    Done,
}

pub struct OptState {
    pub step: OptStep,
    pub scope: Scope,
    /// Folders for the cleanup scan; empty = the configured default (all drives, system folders).
    pub scan_text: String,
    pub dupe_text: String,
    pub progress: Arc<OptProgress>,
    pub cancel: Arc<AtomicBool>,
    pub started: Instant,
    pub analysis: Option<Arc<Analysis>>,
    pub analyze_job: Option<Receiver<Analysis>>,
    pub run_job: Option<Receiver<RunReport>>,
    /// The user ticked "I approve this list" on the review step.
    pub approved: bool,
    pub report: Option<RunReport>,
}

impl Default for OptState {
    fn default() -> Self {
        Self {
            step: OptStep::Scope,
            scope: Scope::default(),
            scan_text: String::new(),
            dupe_text: "~".into(),
            progress: Arc::new(OptProgress::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            started: Instant::now(),
            analysis: None,
            analyze_job: None,
            run_job: None,
            approved: false,
            report: None,
        }
    }
}

impl OptState {
    pub fn busy(&self) -> bool {
        self.analyze_job.is_some() || self.run_job.is_some()
    }
}

fn parse_paths(text: &str) -> Vec<PathBuf> {
    text.split(';').map(str::trim).filter(|s| !s.is_empty()).map(draxbyte::config::expand_path).collect()
}

impl App {
    /// Step 1 → 2: measures what the chosen parts would do. Changes nothing.
    pub fn opt_analyze(&mut self, ctx: &egui::Context) {
        if self.opt.busy() || self.scan.is_some() || self.clean_job.is_some() {
            if self.scan.is_some() || self.clean_job.is_some() {
                self.toast(t("Önce çalışan tarama/temizlik bitsin", "Wait for the running scan/cleanup to finish"));
            }
            return;
        }
        let mut scope = self.opt.scope.clone();
        scope.scan_roots = Some(parse_paths(&self.opt.scan_text)).filter(|v| !v.is_empty());
        scope.dupe_roots = parse_paths(&self.opt.dupe_text);
        if !scope.any() {
            self.toast(t("En az bir işlem seç", "Pick at least one task"));
            return;
        }
        if scope.dupes && scope.dupe_roots.is_empty() {
            self.toast(t("Kopya araması için bir klasör yaz", "Enter a folder for the duplicate search"));
            return;
        }
        self.opt.scope = scope.clone();
        self.opt.progress = Arc::new(OptProgress::default());
        self.opt.cancel = Arc::new(AtomicBool::new(false));
        self.opt.analysis = None;
        self.opt.report = None;
        self.opt.approved = false;
        self.opt.started = Instant::now();
        let (cfg, progress, cancel, ctx) = (self.cfg.clone(), self.opt.progress.clone(), self.opt.cancel.clone(), ctx.clone());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(optimize::analyze(&cfg, &scope, &cancel, &progress));
            ctx.request_repaint();
        });
        self.opt.analyze_job = Some(rx);
        self.opt.step = OptStep::Analyzing;
    }

    /// Step 3 → 4. Caller must have the user's approval (`opt.approved`).
    pub fn opt_apply(&mut self, ctx: &egui::Context) {
        let Some(an) = self.opt.analysis.clone() else { return };
        if self.opt.run_job.is_some() || !self.opt.approved {
            return;
        }
        let scope = self.opt.scope.clone();
        self.opt.progress = Arc::new(OptProgress::default());
        self.opt.cancel = Arc::new(AtomicBool::new(false));
        self.opt.started = Instant::now();
        let (progress, cancel, ctx) = (self.opt.progress.clone(), self.opt.cancel.clone(), ctx.clone());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // permanent = false: files go to the Recycle Bin; only the bin step deletes for good
            let _ = tx.send(optimize::apply(&an, &scope, false, &cancel, &progress));
            ctx.request_repaint();
        });
        self.opt.run_job = Some(rx);
        self.opt.step = OptStep::Running;
    }

    pub fn opt_cancel(&self) {
        self.opt.cancel.store(true, Relaxed);
    }

    pub fn opt_restart(&mut self) {
        if !self.opt.busy() {
            self.opt = OptState { scope: self.opt.scope.clone(), scan_text: self.opt.scan_text.clone(), dupe_text: self.opt.dupe_text.clone(), ..Default::default() };
        }
    }

    /// Puts back what the run moved to the Recycle Bin (impossible once the bin was emptied).
    pub fn opt_undo(&mut self) {
        let Some(rep) = &self.opt.report else { return };
        let mut restored = 0;
        for cr in &rep.clean_reports {
            match draxbyte::core::cleaner::undo(cr) {
                Ok(n) => restored += n,
                Err(e) => {
                    self.toast(tf!("Geri alma başarısız: {e}", "Undo failed: {e}"));
                    return;
                }
            }
        }
        if restored == 0 {
            self.toast(t("Geri alınacak öğe bulunamadı (Çöp Kutusu boşaltılmış olabilir)", "Nothing to restore (the Recycle Bin may have been emptied)"));
        } else {
            self.toast(tf!("{restored} öğe Çöp Kutusu'ndan geri getirildi", "{restored} item(s) restored from the Recycle Bin"));
        }
    }

    /// Called every frame from `panels_poll`.
    pub fn opt_poll(&mut self) {
        if let Some(rx) = &self.opt.analyze_job {
            if let Ok(an) = rx.try_recv() {
                self.opt.analyze_job = None;
                if an.cancelled {
                    self.opt.step = OptStep::Scope;
                    self.toast(t("Analiz iptal edildi, hiçbir şey değişmedi", "Analysis cancelled, nothing was changed"));
                } else {
                    self.last_errors = None;
                    self.opt.analysis = Some(Arc::new(an));
                    self.opt.step = OptStep::Review;
                }
            }
        }
        if let Some(rx) = &self.opt.run_job {
            if let Ok(rep) = rx.try_recv() {
                self.opt.run_job = None;
                let gone: std::collections::HashSet<&PathBuf> = rep
                    .clean_reports
                    .iter()
                    .flat_map(|c| c.entries.iter())
                    .filter(|e| matches!(e.outcome, Outcome::Trashed | Outcome::Deleted))
                    .map(|e| &e.path)
                    .collect();
                self.rows.retain(|r| !gone.contains(&r.item.path));
                self.view_dirty = true;
                if let Some(db) = &mut self.db {
                    for c in &rep.clean_reports {
                        let _ = db.log_clean(c);
                    }
                }
                self.disk = crate::app::detect_disk();
                self.bin.status = None;
                self.docker = None;
                self.toast(tf!("Optimizasyon bitti · {} açıldı", "Optimization finished · {} freed", human_bytes(rep.freed())));
                self.opt.report = Some(rep);
                self.opt.step = OptStep::Done;
            }
        }
    }
}

/// Title and one-line description of a part (shared by the scope, review and result steps).
pub fn task_title(task: Task) -> &'static str {
    match task {
        Task::Cleanup => t("Güvenli temizlik", "Safe cleanup"),
        Task::Dupes => t("Kopya dosyalar", "Duplicate files"),
        Task::Docker => t("Docker temizliği", "Docker clean-up"),
        Task::DockerCompact => t("Docker disk imajını küçült", "Shrink the Docker disk image"),
        Task::RecycleBin => t("Çöp Kutusunu boşalt", "Empty the Recycle Bin"),
        Task::Memory => t("Belleği boşalt", "Free memory"),
    }
}

pub fn status_text(s: &StepStatus) -> String {
    match s {
        StepStatus::Done => t("tamam", "done").to_string(),
        StepStatus::Skipped(why) => format!("{} — {why}", t("atlandı", "skipped")),
        StepStatus::Failed(e) => format!("{} — {e}", t("başarısız", "failed")),
    }
}
