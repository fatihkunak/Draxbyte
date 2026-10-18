//! State and background jobs of the Programs and Memory screens. The screens themselves are in
//! `ui/apps_view.rs` and `ui/system_view.rs`; everything slow runs on plain threads and reports
//! back over channels that `App::poll_jobs` drains.

use crate::app::App;
use draxbyte::core::apps::{self, Confidence, InstalledApp, Leftover, RemoveOptions, RemoveReport, Removability};
use draxbyte::core::memory::{self, MemInfo, ProcGroup, TrimReport};
use draxbyte::core::startup::{self, StartupEntry};
use draxbyte::i18n::t;
use draxbyte::tf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppSort {
    Size,
    LastUsed,
    Name,
}

pub struct AppsState {
    pub list: Vec<InstalledApp>,
    pub loaded: bool,
    pub job: Option<Receiver<Vec<InstalledApp>>>,
    pub filter: String,
    pub only_unused: bool,
    pub unused_days: i64,
    pub show_hidden: bool,
    pub sort: AppSort,
    pub flow: Option<Flow>,
    /// Backup folder of the last removal, for the "restore registry" button.
    pub last_backup: Option<std::path::PathBuf>,
    pub auto_opened: bool,
}

impl Default for AppsState {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            loaded: false,
            job: None,
            filter: String::new(),
            only_unused: false,
            unused_days: 90,
            show_hidden: false,
            sort: AppSort::Size,
            flow: None,
            last_backup: None,
            auto_opened: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlowStage {
    /// Showing what would happen; waiting for the user.
    Review,
    /// The uninstaller / removal is running.
    Working,
    /// The uninstaller finished; the user picks which leftovers go.
    Cleanup,
    Done,
}

pub enum FlowMsg {
    Preview(Vec<Leftover>, Vec<String>),
    Log(String),
    Leftovers(Vec<Leftover>),
    Done(RemoveReport, Option<String>),
}

pub struct Flow {
    pub app: InstalledApp,
    pub others: Vec<InstalledApp>,
    pub stage: FlowStage,
    /// Skip the vendor's uninstaller and delete by force.
    pub force: bool,
    pub silent: bool,
    pub permanent: bool,
    pub close_running: bool,
    pub preview: Option<Vec<Leftover>>,
    pub running: Vec<String>,
    /// Leftovers with their tick box.
    pub items: Vec<(Leftover, bool)>,
    pub log: Vec<String>,
    pub report: Option<RemoveReport>,
    pub after_note: Option<String>,
    pub rx: Option<Receiver<FlowMsg>>,
    pub restore_msg: Option<String>,
    /// "Probable" items are ticked: the user must confirm those once more before anything is deleted.
    pub confirm_probable: bool,
}

impl Flow {
    fn new(app: InstalledApp, others: Vec<InstalledApp>) -> Self {
        // A program whose vendor hides the uninstaller, or has none, is removed by force by default.
        let force = app.uninstall_cmd.is_none();
        Flow {
            app,
            others,
            stage: FlowStage::Review,
            force,
            silent: false,
            permanent: true,
            close_running: true,
            preview: None,
            running: Vec::new(),
            items: Vec::new(),
            log: Vec::new(),
            report: None,
            after_note: None,
            rx: None,
            restore_msg: None,
            confirm_probable: false,
        }
    }

    /// Ticked items that are only probable matches (shared vendor folders, profiles…).
    pub fn probable_selected(&self) -> Vec<&Leftover> {
        self.items.iter().filter(|(l, on)| *on && l.confidence == Confidence::Medium).map(|(l, _)| l).collect()
    }

    pub fn selected_bytes(&self) -> u64 {
        self.items.iter().filter(|(_, on)| *on).map(|(l, _)| l.size).sum()
    }
}

pub struct SysState {
    pub mem: Option<MemInfo>,
    pub groups: Vec<ProcGroup>,
    pub job: Option<Receiver<(MemInfo, Vec<ProcGroup>)>>,
    pub last: Option<Instant>,
    pub startup: Vec<StartupEntry>,
    pub startup_loaded: bool,
    pub trim_job: Option<Receiver<Result<TrimReport, String>>>,
    pub trim_result: Option<Result<TrimReport, String>>,
    /// (group index, force) waiting for the second click.
    pub confirm_close: Option<(usize, bool)>,
    pub msg: Option<String>,
    pub filter: String,
}

impl Default for SysState {
    fn default() -> Self {
        Self {
            mem: None,
            groups: Vec::new(),
            job: None,
            last: None,
            startup: Vec::new(),
            startup_loaded: false,
            trim_job: None,
            trim_result: None,
            confirm_close: None,
            msg: None,
            filter: String::new(),
        }
    }
}

impl App {
    // ---- programs ------------------------------------------------------------------------------

    pub fn apps_refresh(&mut self, ctx: &egui::Context) {
        if self.apps.job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut list = apps::inventory(true);
            apps::fill_missing_sizes(&mut list, &AtomicBool::new(false));
            let _ = tx.send(list);
            ctx.request_repaint();
        });
        self.apps.job = Some(rx);
    }

    /// Opens the removal dialog for `list[idx]` and starts computing what it would touch.
    pub fn flow_open(&mut self, ctx: &egui::Context, idx: usize) {
        let Some(app) = self.apps.list.get(idx).cloned() else { return };
        let others: Vec<InstalledApp> = self.apps.list.iter().filter(|a| a.id != app.id).cloned().collect();
        let (tx, rx) = channel();
        let (a2, o2, ctx2) = (app.clone(), others.clone(), ctx.clone());
        std::thread::spawn(move || {
            let running: Vec<String> = a2
                .install_location
                .as_deref()
                .map(apps::processes_under)
                .unwrap_or_default()
                .into_iter()
                .map(|g| format!("{} ({})", g.name, g.pids.len()))
                .collect();
            let _ = tx.send(FlowMsg::Preview(apps::find_leftovers(&a2, &o2), running));
            ctx2.request_repaint();
        });
        let mut flow = Flow::new(app, others);
        flow.rx = Some(rx);
        self.apps.flow = Some(flow);
    }

    /// Step 1: close the program, restore point, vendor uninstaller (unless forced), then rescan.
    pub fn flow_run(&mut self, ctx: &egui::Context) {
        let Some(f) = self.apps.flow.as_mut() else { return };
        f.stage = FlowStage::Working;
        f.log.clear();
        let (app, others, force, silent, close) = (f.app.clone(), f.others.clone(), f.force, f.silent, f.close_running);
        let (tx, rx) = channel();
        f.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let log = |m: String| {
                let _ = tx.send(FlowMsg::Log(m));
                ctx.request_repaint();
            };
            if close {
                let r = apps::close_running(&app);
                if r.asked > 0 {
                    log(tf!("{} çalışan işlem kapatıldı", "{} running processes closed", r.asked));
                }
            }
            match apps::create_restore_point(&app.name) {
                Ok(()) => log(t("Geri yükleme noktası oluşturuldu", "Restore point created").to_string()),
                Err(e) => log(tf!("Geri yükleme noktası yok ({e}); devam ediliyor", "No restore point ({e}); continuing")),
            }
            if !force {
                log(t("Üreticinin kaldırıcısı çalışıyor…", "The vendor's uninstaller is running…").to_string());
                let by_force_flag = matches!(app.removable, Removability::Blocked(_));
                match apps::run_uninstaller(&app, silent, by_force_flag) {
                    Ok(code) => log(tf!("Kaldırıcı bitti (çıkış kodu {})", "Uninstaller finished (exit code {})", code)),
                    Err(e) => log(tf!("Kaldırıcı çalışmadı: {e}", "Uninstaller did not run: {e}")),
                }
                // Many uninstallers hand over to a copy of themselves and return at once.
                for _ in 0..45 {
                    if !apps::still_registered(&app) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
            log(t("Artıklar aranıyor…", "Looking for leftovers…").to_string());
            let _ = tx.send(FlowMsg::Leftovers(apps::find_leftovers(&app, &others)));
            ctx.request_repaint();
        });
    }

    /// Step 2: delete the ticked leftovers (registry exported first).
    pub fn flow_clean(&mut self, ctx: &egui::Context) {
        let Some(f) = self.apps.flow.as_mut() else { return };
        f.confirm_probable = false;
        let chosen: Vec<Leftover> = f.items.iter().filter(|(_, on)| *on).map(|(l, _)| l.clone()).collect();
        let (app, permanent) = (f.app.clone(), f.permanent);
        f.stage = FlowStage::Working;
        let (tx, rx) = channel();
        f.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let opts = RemoveOptions { permanent, backup_dir: apps::backup_dir_for(&app) };
            let rep = apps::remove_leftovers(&app, &chosen, &opts);
            let note = apps::after_removal(&app).map(|r| match r {
                Ok(m) => m,
                Err(e) => tf!("Ek adım yapılamadı ({e}) — yönetici olarak çalıştır", "Extra step failed ({e}) — run as administrator"),
            });
            let _ = tx.send(FlowMsg::Done(rep, note));
            ctx.request_repaint();
        });
    }

    /// Drains messages of the open removal dialog.
    pub fn flow_poll(&mut self) {
        let mut finished = false;
        if let Some(f) = self.apps.flow.as_mut() {
            let mut msgs = Vec::new();
            if let Some(rx) = &f.rx {
                while let Ok(m) = rx.try_recv() {
                    msgs.push(m);
                }
            }
            for m in msgs {
                match m {
                    FlowMsg::Preview(items, running) => {
                        f.preview = Some(items);
                        f.running = running;
                    }
                    FlowMsg::Log(l) => f.log.push(l),
                    FlowMsg::Leftovers(items) => {
                        f.items = items.into_iter().map(|l| { let on = l.confidence == Confidence::High; (l, on) }).collect();
                        f.stage = FlowStage::Cleanup;
                    }
                    FlowMsg::Done(rep, note) => {
                        self.apps.last_backup = Some(rep.backup_dir.clone());
                        f.report = Some(rep);
                        f.after_note = note;
                        f.stage = FlowStage::Done;
                        finished = true;
                    }
                }
            }
        }
        if finished {
            // The list changed: reload it in the background next time the screen draws.
            self.apps.loaded = false;
        }
    }

    pub fn flow_close(&mut self) {
        self.apps.flow = None;
    }

    // ---- memory & startup ------------------------------------------------------------------------

    pub fn sys_refresh(&mut self, ctx: &egui::Context) {
        if self.sys.job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(memory::snapshot());
            ctx.request_repaint();
        });
        self.sys.job = Some(rx);
        self.sys.last = Some(Instant::now());
    }

    pub fn sys_trim(&mut self, ctx: &egui::Context) {
        if self.sys.trim_job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(memory::trim_working_sets());
            ctx.request_repaint();
        });
        self.sys.trim_job = Some(rx);
    }

    /// Closes the programs of `groups[idx]`; `force` ends them without asking the program.
    pub fn sys_close(&mut self, idx: usize, force: bool) {
        let Some(g) = self.sys.groups.get(idx) else { return };
        let (name, pids) = (g.name.clone(), g.pids.clone());
        if self.is_user_protected(&name) {
            self.sys.msg = Some(tf!("{} koruma listende; kapatılmadı", "{} is on your protected list; not closed", name));
            return;
        }
        let rep = memory::close_pids(&pids, force);
        self.sys.msg = Some(if rep.refused.len() == pids.len() {
            tf!("{} bir sistem programı; kapatılmadı", "{} is a system program; not closed", name)
        } else if rep.failed.is_empty() {
            tf!("{}: kapatma isteği gönderildi", "{}: close request sent", name)
        } else {
            tf!("{}: {} işlem kapanmadı", "{}: {} processes did not close", name, rep.failed.len())
        });
        self.sys.last = None; // refresh on the next draw
    }

    pub fn is_user_protected(&self, name: &str) -> bool {
        self.cfg.memory.protected.iter().any(|p| p.eq_ignore_ascii_case(name))
    }

    /// Adds/removes a program name to the "never close" list and saves the config.
    pub fn set_user_protected(&mut self, name: &str, on: bool) {
        let list = &mut self.cfg.memory.protected;
        list.retain(|p| !p.eq_ignore_ascii_case(name));
        if on {
            list.push(name.to_string());
        }
        self.cfg_draft.memory.protected = self.cfg.memory.protected.clone();
        if let Err(e) = self.cfg.save(&self.cfg_path) {
            self.sys.msg = Some(tf!("Ayar kaydedilemedi: {e}", "Could not save the setting: {e}"));
        }
        self.sys.confirm_close = None;
    }

    pub fn startup_reload(&mut self) {
        self.sys.startup = startup::list();
        self.sys.startup_loaded = true;
    }

    pub fn startup_toggle(&mut self, idx: usize, enabled: bool) {
        let Some(e) = self.sys.startup.get(idx).cloned() else { return };
        match startup::set_enabled(&e, enabled) {
            Ok(()) => self.sys.msg = None,
            Err(m) => self.sys.msg = Some(tf!("{}: {m}", "{}: {m}", e.name)),
        }
        self.startup_reload();
    }

    /// Called every frame from `poll_jobs`.
    pub fn panels_poll(&mut self) {
        self.map_dupes_poll();
        if let Some(rx) = &self.apps.job {
            if let Ok(list) = rx.try_recv() {
                self.apps.job = None;
                self.apps.list = list;
                self.apps.loaded = true;
            }
        }
        self.flow_poll();
        if let Some(rx) = &self.sys.job {
            if let Ok((m, g)) = rx.try_recv() {
                self.sys.job = None;
                self.sys.mem = Some(m);
                self.sys.groups = g;
            }
        }
        if let Some(rx) = &self.sys.trim_job {
            if let Ok(r) = rx.try_recv() {
                self.sys.trim_job = None;
                self.sys.trim_result = Some(r);
                self.sys.last = None;
            }
        }
    }
}
