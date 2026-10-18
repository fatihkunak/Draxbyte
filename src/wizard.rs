//! Setup and uninstall wizards. The setup file is this same program started in setup mode
//! (`Draxbyte-Setup.exe`, or `Draxbyte.app` opened from the disk image), so the wizard has the
//! normal look, logo and languages. The work itself is in `draxbyte::core::installer`.
//!
//! Windows: per-user install into `%LOCALAPPDATA%\Programs\Draxbyte`, shortcuts, "Apps &
//! features" entry; uninstall runs from a temporary copy so it can delete its own folder.
//! macOS: copy `Draxbyte.app` into /Applications; uninstall removes it (and data if asked).

use crate::ui::{self, widgets, ACCENT, MUTED, WARN};
use anyhow::Result;
use draxbyte::core::installer::{self, Existing, InstallOptions, Target, UninstallOptions};
use draxbyte::i18n::t;
use draxbyte::tf;
use egui::RichText;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Setup,
    Uninstall,
}

/// Which wizard (if any) this start should show.
pub fn detect_mode(args: &[String], exe: &Path) -> Option<Mode> {
    if args.iter().any(|a| a == "--uninstall") {
        return Some(Mode::Uninstall);
    }
    if args.iter().any(|a| a == "--setup") {
        return Some(Mode::Setup);
    }
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    if cfg!(windows) && stem.contains("setup") {
        return Some(Mode::Setup);
    }
    // macOS: Draxbyte.app opened straight from the mounted disk image
    if cfg!(target_os = "macos") && exe.to_string_lossy().starts_with("/Volumes/") {
        return Some(Mode::Setup);
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Welcome,
    License,
    Options,
    Working,
    Done,
}

enum Msg {
    Log(String),
    Finished(Result<(), String>),
}

struct Wizard {
    mode: Mode,
    page: Page,
    existing: Option<Existing>,
    license_ok: bool,
    dest_text: String,
    desktop: bool,
    start_menu: bool,
    launch_after: bool,
    remove_data: bool,
    data_size: u64,
    log: Vec<String>,
    rx: Option<Receiver<Msg>>,
    result: Option<Result<(), String>>,
    logo: egui::TextureHandle,
    target: Target,
    /// Uninstall from a temporary copy: delete it when done.
    temp_copy: Option<PathBuf>,
}

impl Wizard {
    fn new(cc: &eframe::CreationContext<'_>, mode: Mode, dir_override: Option<PathBuf>, temp_copy: Option<PathBuf>) -> Self {
        ui::apply_theme(&cc.egui_ctx);
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([256, 256], ui::ICON_RGBA),
            egui::TextureOptions::LINEAR,
        );
        // DRAXBYTE_WIZARD_SANDBOX=<folder>: run the wizard against scratch locations only.
        let sandbox = std::env::var_os("DRAXBYTE_WIZARD_SANDBOX").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty());
        let target = sandbox.as_deref().map(Target::sandbox).unwrap_or_else(Target::real);
        let existing = installer::existing(&target).or_else(|| {
            dir_override.clone().map(|dir| Existing { dir, version: String::new() })
        });
        let default_dest = sandbox.as_ref().map(|s| s.join("Programs").join("Draxbyte")).unwrap_or_else(installer::default_dir);
        let dest = existing.as_ref().map(|e| e.dir.clone()).unwrap_or(default_dest);
        let (desktop, start_menu) = if existing.is_some() {
            (target.desktop_dir.join("Draxbyte.lnk").exists(), target.start_menu_dir.join("Draxbyte.lnk").exists())
        } else {
            (true, true)
        };
        Wizard {
            mode,
            page: Page::Welcome,
            existing,
            license_ok: false,
            dest_text: dest.to_string_lossy().into_owned(),
            desktop,
            start_menu,
            launch_after: true,
            remove_data: false,
            data_size: installer::dir_size(&target.data_dir),
            log: Vec::new(),
            rx: None,
            result: None,
            logo,
            target,
            temp_copy,
        }
    }

    fn is_update(&self) -> bool {
        self.mode == Mode::Setup && self.existing.is_some()
    }

    /// The pages of this wizard, in order.
    fn pages(&self) -> Vec<(Page, &'static str)> {
        match self.mode {
            Mode::Uninstall => vec![
                (Page::Welcome, t("Onay", "Confirm")),
                (Page::Working, t("Kaldırma", "Removing")),
                (Page::Done, t("Bitti", "Done")),
            ],
            Mode::Setup if self.is_update() => vec![
                (Page::Welcome, t("Güncelleme", "Update")),
                (Page::Working, t("Kurulum", "Installing")),
                (Page::Done, t("Bitti", "Done")),
            ],
            Mode::Setup => vec![
                (Page::Welcome, t("Hoş geldin", "Welcome")),
                (Page::License, t("Lisans", "License")),
                (Page::Options, t("Seçenekler", "Options")),
                (Page::Working, t("Kurulum", "Installing")),
                (Page::Done, t("Bitti", "Done")),
            ],
        }
    }

    fn dest(&self) -> PathBuf {
        PathBuf::from(self.dest_text.trim())
    }

    fn next(&mut self, ctx: &egui::Context) {
        let pages = self.pages();
        let i = pages.iter().position(|(p, _)| *p == self.page).unwrap_or(0);
        if let Some((p, _)) = pages.get(i + 1) {
            self.page = *p;
            if *p == Page::Working {
                self.start_work(ctx);
            }
        }
    }

    fn back(&mut self) {
        let pages = self.pages();
        let i = pages.iter().position(|(p, _)| *p == self.page).unwrap_or(0);
        if i > 0 {
            self.page = pages[i - 1].0;
        }
    }

    fn start_work(&mut self, ctx: &egui::Context) {
        self.log.clear();
        self.result = None;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let (mode, target) = (self.mode, self.target.clone());
        let dest = self.dest();
        let (desktop, start_menu, remove_data) = (self.desktop, self.start_menu, self.remove_data);
        let src = std::env::current_exe().unwrap_or_default();
        std::thread::spawn(move || {
            let mut log = |m: String| {
                let _ = tx.send(Msg::Log(m));
                ctx.request_repaint();
            };
            let res = match mode {
                Mode::Setup => installer::install(&src, &InstallOptions { dest, desktop_shortcut: desktop, start_menu_shortcut: start_menu }, &target, &mut log),
                Mode::Uninstall => installer::uninstall(&UninstallOptions { dir: dest, remove_data }, &target, std::process::id(), &mut log),
            };
            let _ = tx.send(Msg::Finished(res));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Msg::Log(l) => self.log.push(l),
                Msg::Finished(r) => {
                    self.result = Some(r);
                    self.page = Page::Done;
                    self.rx = None;
                }
            }
        }
    }

    fn launch_installed(&self) {
        let dest = self.dest();
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(dest.join("Draxbyte.app")).spawn();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = std::process::Command::new(dest.join(installer::EXE_NAME)).spawn();
        }
    }

    /// After an uninstall run from a temporary copy: delete that copy once we have exited.
    fn schedule_temp_cleanup(&self) {
        #[cfg(windows)]
        if let Some(p) = &self.temp_copy {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd.exe")
                .args(["/C", &format!("ping -n 3 127.0.0.1 >nul & del /f /q \"{}\"", p.display())])
                .creation_flags(0x0800_0000 | 0x0000_0008) // no window, detached
                .spawn();
        }
    }
}

impl eframe::App for Wizard {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        let working = self.page == Page::Working;

        // ---- brand rail with the steps
        egui::Panel::left("rail")
            .exact_size(210.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(ui::SIDEBAR).inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| {
                widgets::logo(ui, &self.logo);
                ui.add_space(22.0);
                let pages = self.pages();
                let cur = pages.iter().position(|(p, _)| *p == self.page).unwrap_or(0);
                for (i, (_, label)) in pages.iter().enumerate() {
                    let (mark, color) = if i < cur {
                        ("✓", ACCENT)
                    } else if i == cur {
                        ("●", ACCENT)
                    } else {
                        ("○", MUTED)
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(mark).color(color));
                        ui.label(RichText::new(*label).color(if i == cur { ui::TEXT } else { MUTED }));
                    });
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(MUTED).small());
                });
            });

        // ---- button bar
        egui::Panel::bottom("buttons")
            .frame(egui::Frame::new().fill(ui::BG).inner_margin(egui::Margin::symmetric(24, 14)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.buttons(ui, &ctx, working));
                });
            });

        // ---- page
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui::BG).inner_margin(egui::Margin::same(26)))
            .show(ui, |ui| match (self.mode, self.page) {
                (_, Page::Welcome) => self.welcome(ui),
                (_, Page::License) => self.license(ui),
                (_, Page::Options) => self.options(ui),
                (_, Page::Working) => self.working(ui),
                (_, Page::Done) => self.done(ui),
            });
        if working {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

impl Wizard {
    fn buttons(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, working: bool) {
        let first = self.pages().first().map(|(p, _)| *p) == Some(self.page);
        match self.page {
            Page::Done => {
                let ok = matches!(self.result, Some(Ok(())));
                if ui.add(widgets::primary_button(if ok && self.mode == Mode::Setup && self.launch_after { t("Bitir ve aç", "Finish and open") } else { t("Bitir", "Finish") })).clicked() {
                    if ok && self.mode == Mode::Setup && self.launch_after {
                        self.launch_installed();
                    }
                    self.schedule_temp_cleanup();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if !ok && ui.button(t("Tekrar dene", "Try again")).clicked() {
                    self.page = if self.mode == Mode::Setup && !self.is_update() { Page::Options } else { Page::Welcome };
                }
            }
            Page::Working => {
                ui.add_enabled(false, egui::Button::new(t("Çalışıyor…", "Working…")));
            }
            _ => {
                let (label, can) = match (self.mode, self.page) {
                    (Mode::Uninstall, Page::Welcome) => (t("Kaldır", "Uninstall"), true),
                    (Mode::Setup, Page::Welcome) if self.is_update() => (t("Güncelle", "Update"), true),
                    (Mode::Setup, Page::Options) => (t("Kur", "Install"), installer::validate_dir(&self.dest()).is_ok()),
                    (Mode::Setup, Page::License) => (t("İleri", "Next"), self.license_ok),
                    _ => (t("İleri", "Next"), true),
                };
                let btn = if self.mode == Mode::Uninstall { widgets::danger_button(label) } else { widgets::primary_button(label) };
                if ui.add_enabled(can, btn).clicked() {
                    self.next(ctx);
                }
                if !first && ui.button(t("Geri", "Back")).clicked() {
                    self.back();
                }
                if ui.button(t("İptal", "Cancel")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        let _ = working;
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        match self.mode {
            Mode::Uninstall => {
                ui.label(RichText::new(t("Draxbyte kaldırılsın mı?", "Remove Draxbyte?")).font(ui::bold(26.0)));
                ui.add_space(6.0);
                if let Some(e) = &self.existing {
                    ui.label(RichText::new(tf!("Konum: {}", "Location: {}", e.dir.display())).color(MUTED));
                }
                ui.add_space(14.0);
                ui.label(t(
                    "Program, kısayolları ve \"Uygulamalar ve özellikler\" kaydı kaldırılır. Draxbyte'ın temizlediği ya da taradığın hiçbir dosyaya dokunulmaz.",
                    "The program, its shortcuts and its \"Apps & features\" entry are removed. No file you scanned or cleaned is touched.",
                ));
                ui.add_space(12.0);
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.checkbox(
                        &mut self.remove_data,
                        tf!(
                            "Ayarları ve verileri de sil ({})",
                            "Also delete settings and data ({})",
                            draxbyte::models::human_bytes(self.data_size)
                        ),
                    );
                    ui.label(
                        RichText::new(t(
                            "Ayarlar, tarama geçmişi, AI önbelleği ve program kaldırma yedekleri (kayıt defteri .reg dosyaları) ~/.draxbyte içindedir. İşaretlemezsen kalır; sonradan yeniden kurarsan bulunur.",
                            "Settings, scan history, the AI cache and the uninstall backups (registry .reg files) live in ~/.draxbyte. Leave it unticked to keep them for a reinstall.",
                        ))
                        .color(MUTED)
                        .small(),
                    );
                });
            }
            Mode::Setup => {
                let upd = self.is_update();
                ui.label(
                    RichText::new(if upd { t("Draxbyte güncellenecek", "Draxbyte will be updated") } else { t("Draxbyte'a hoş geldin", "Welcome to Draxbyte") })
                        .font(ui::bold(26.0)),
                );
                ui.add_space(6.0);
                if let Some(e) = &self.existing {
                    let from = if e.version.is_empty() { String::new() } else { format!("{} → ", e.version) };
                    ui.label(RichText::new(format!("{from}{}", env!("CARGO_PKG_VERSION"))).color(ACCENT).font(ui::bold(16.0)));
                }
                ui.add_space(14.0);
                ui.label(t(
                    "Draxbyte diskini yerelde tarar; büyük, eski ve kopya dosyaları, kullanılmayan programları ve gereksiz sistem artıklarını bulur. Hiçbir şeyi sen onaylamadan silmez.",
                    "Draxbyte scans your disk locally and finds large, old and duplicate files, unused programs and system leftovers. It never deletes anything without your approval.",
                ));
                ui.add_space(12.0);
                for (a, b) in [
                    (t("Yerel ve gizli", "Local and private"), t("Tarama ve kararlar bu bilgisayarda; telemetri yok.", "Scanning and decisions happen on this computer; no telemetry.")),
                    (t("Güvenli", "Safe"), t("Sistem klasörleri korunur; silmeden hemen önce yeniden kontrol edilir.", "System folders are protected; everything is re-checked right before deletion.")),
                    (t("Geri alınabilir", "Reversible"), t("Çöp Kutusu varsayılan; kayıt defteri önce yedeklenir.", "Recycle Bin by default; the registry is exported first.")),
                ] {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("•").color(ACCENT));
                        ui.label(RichText::new(a).strong());
                        ui.label(RichText::new(b).color(MUTED));
                    });
                }
                ui.add_space(12.0);
                if cfg!(windows) {
                    ui.label(RichText::new(t("Yönetici izni gerekmez; yalnızca senin kullanıcın için kurulur.", "No administrator rights needed; installed for your user only.")).color(MUTED).small());
                } else {
                    ui.label(RichText::new(t("Draxbyte.app Applications klasörüne kopyalanır. İlk açılışta macOS onay isterse: uygulamaya sağ tık → Aç.", "Draxbyte.app is copied into Applications. If macOS asks on the first start: right-click the app → Open.")).color(MUTED).small());
                }
            }
        }
    }

    fn license(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(t("Lisans", "License")).font(ui::bold(24.0)));
        ui.add_space(6.0);
        ui.label(RichText::new(t("Draxbyte açık kaynaktır (MIT).", "Draxbyte is open source (MIT).")).color(MUTED));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(include_str!("../LICENSE")).small().color(MUTED)).wrap());
        });
        ui.add_space(10.0);
        ui.checkbox(&mut self.license_ok, t("Lisansı okudum ve kabul ediyorum", "I have read and accept the license"));
    }

    fn options(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(t("Kurulum seçenekleri", "Install options")).font(ui::bold(24.0)));
        ui.add_space(10.0);
        ui.label(RichText::new(if cfg!(target_os = "macos") { t("Kurulacak klasör (içine Draxbyte.app konur)", "Folder to install into (Draxbyte.app goes inside)") } else { t("Kurulacak klasör", "Install folder") }).color(MUTED));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.dest_text).desired_width((ui.available_width() - 110.0).max(150.0)));
            if ui.button(t("Varsayılan", "Default")).clicked() {
                self.dest_text = installer::default_dir().to_string_lossy().into_owned();
            }
        });
        match installer::validate_dir(&self.dest()) {
            Ok(()) => {}
            Err(e) => {
                ui.label(RichText::new(e).color(WARN).small());
            }
        }
        ui.add_space(14.0);
        if cfg!(windows) {
            ui.checkbox(&mut self.desktop, t("Masaüstü kısayolu oluştur", "Create a desktop shortcut"));
            ui.checkbox(&mut self.start_menu, t("Başlat menüsü kısayolu oluştur", "Create a Start menu shortcut"));
        }
        ui.checkbox(&mut self.launch_after, t("Kurulum bitince Draxbyte'ı aç", "Open Draxbyte when the installation finishes"));
    }

    fn working(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(match self.mode {
                Mode::Setup => t("Kuruluyor…", "Installing…"),
                Mode::Uninstall => t("Kaldırılıyor…", "Removing…"),
            })
            .font(ui::bold(24.0)),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t("Lütfen bekle, bu pencereyi kapatma.", "Please wait and keep this window open."));
        });
        ui.add_space(10.0);
        for l in &self.log {
            ui.label(RichText::new(format!("✓ {l}")).color(MUTED));
        }
    }

    fn done(&mut self, ui: &mut egui::Ui) {
        match &self.result {
            Some(Ok(())) => {
                ui.label(
                    RichText::new(match self.mode {
                        Mode::Setup if self.is_update() => t("Güncellendi", "Updated"),
                        Mode::Setup => t("Kurulum tamamlandı", "Installation complete"),
                        Mode::Uninstall => t("Draxbyte kaldırıldı", "Draxbyte was removed"),
                    })
                    .font(ui::bold(26.0))
                    .color(ACCENT),
                );
                ui.add_space(10.0);
                match self.mode {
                    Mode::Setup => {
                        ui.label(RichText::new(tf!("Konum: {}", "Location: {}", self.dest().display())).color(MUTED));
                        ui.add_space(8.0);
                        ui.checkbox(&mut self.launch_after, t("Draxbyte'ı şimdi aç", "Open Draxbyte now"));
                        if cfg!(windows) && self.log.iter().any(|l| l.contains(".old") || l.contains("yeniden adlandırıl") || l.contains("renamed")) {
                            ui.label(RichText::new(t("Açık olan Draxbyte'ı kapatıp yeniden açınca yeni sürüm devreye girer.", "Close and reopen the running Draxbyte to switch to the new version.")).color(WARN).small());
                        }
                    }
                    Mode::Uninstall => {
                        ui.label(RichText::new(t("Güle güle. Fikrin ya da bir sorunun varsa: github.com/fatihkunak/Draxbyte/issues", "Goodbye. Ideas or problems: github.com/fatihkunak/Draxbyte/issues")).color(MUTED));
                    }
                }
                for l in &self.log {
                    ui.label(RichText::new(format!("✓ {l}")).color(MUTED).small());
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(t("Tamamlanamadı", "Could not finish")).font(ui::bold(26.0)).color(ui::DANGER));
                ui.add_space(10.0);
                ui.add(egui::Label::new(RichText::new(e).color(WARN)).wrap());
                ui.add_space(8.0);
                ui.label(RichText::new(t("Draxbyte açıksa kapatıp tekrar dene.", "If Draxbyte is running, close it and try again.")).color(MUTED));
            }
            None => {}
        }
    }
}

/// Starts the wizard. For an uninstall that runs from inside the install folder the program first
/// copies itself to a temporary file and continues from there, so it can delete its own folder.
pub fn run(mode: Mode, args: &[String]) -> Result<()> {
    let exe = std::env::current_exe()?;
    let dir_arg = args.iter().position(|a| a == "--dir").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let mut temp_copy = None;
    if cfg!(windows) && mode == Mode::Uninstall && !args.iter().any(|a| a == "--stage2") {
        let target = Target::real();
        if let Some(ex) = installer::existing(&target) {
            if exe.starts_with(&ex.dir) {
                let tmp = std::env::temp_dir().join(format!("draxbyte-uninstall-{}.exe", std::process::id()));
                std::fs::copy(&exe, &tmp)?;
                std::process::Command::new(&tmp).args(["--uninstall", "--stage2", "--dir"]).arg(&ex.dir).spawn()?;
                return Ok(());
            }
        }
    }
    if args.iter().any(|a| a == "--stage2") {
        temp_copy = Some(exe.clone());
    }
    let icon = egui::IconData { rgba: ui::ICON_RGBA.to_vec(), width: 256, height: 256 };
    let title = match mode {
        Mode::Setup => t("Draxbyte Kurulum", "Draxbyte Setup"),
        Mode::Uninstall => t("Draxbyte Kaldır", "Uninstall Draxbyte"),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 540.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title(title)
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(title, options, Box::new(move |cc| Ok(Box::new(Wizard::new(cc, mode, dir_arg.clone(), temp_copy.clone())))))
        .map_err(|e| anyhow::anyhow!(tf!("Sihirbaz başlatılamadı: {e}", "could not start the wizard: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mode_is_detected_from_flags_and_file_name() {
        let app = Path::new("/x/draxbyte.exe");
        assert_eq!(detect_mode(&a(&[]), app), None);
        assert_eq!(detect_mode(&a(&["--uninstall"]), app), Some(Mode::Uninstall));
        assert_eq!(detect_mode(&a(&["--setup"]), app), Some(Mode::Setup));
        // uninstall wins when both are present, and CLI subcommands are never mistaken for it
        assert_eq!(detect_mode(&a(&["scan"]), app), None);
        if cfg!(windows) {
            assert_eq!(detect_mode(&a(&[]), Path::new(r"C:\Users\u\Downloads\Draxbyte-Setup.exe")), Some(Mode::Setup));
            assert_eq!(detect_mode(&a(&[]), Path::new(r"C:\Users\u\AppData\Local\Programs\Draxbyte\draxbyte.exe")), None);
        }
        if cfg!(target_os = "macos") {
            assert_eq!(detect_mode(&a(&[]), Path::new("/Volumes/Draxbyte/Draxbyte.app/Contents/MacOS/draxbyte")), Some(Mode::Setup));
            assert_eq!(detect_mode(&a(&[]), Path::new("/Applications/Draxbyte.app/Contents/MacOS/draxbyte")), None);
        }
    }
}
