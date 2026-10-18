//! About screen: product, developer, data locations, third-party licences.

use super::widgets::{card, heading};
use super::{ACCENT, MUTED};
use crate::app::App;
use draxbyte::config::data_dir;
use draxbyte::i18n::t;
use draxbyte::tf;
use egui::RichText;

// Edit these to change what the About screen shows.
const DEV_NAME: &str = "Fatih Kunak";
const DEV_SITE: &str = "https://fatihgo.com";
const DEV_GITHUB: &str = "https://github.com/fatihkunak";
const REPO: &str = "https://github.com/fatihkunak/Draxbyte";
const COPYRIGHT_YEAR: &str = "2026";

/// Opens a folder in the OS file manager (the user clicked a button for it).
fn open_folder(dir: &std::path::Path) {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

/// Left-aligned fixed-width label column.
fn label_cell(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(130.0, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(label).color(MUTED));
        },
    );
}

fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal(|ui| {
        label_cell(ui, label);
        ui.label(value.into());
    });
}

/// "Update now" button, inline confirmation, download progress and errors.
fn update_install_row(app: &mut App, ui: &mut egui::Ui) {
    use std::sync::atomic::Ordering::Relaxed;
    if let Some(dl) = &app.update_dl {
        let (done, total) = (dl.progress.done.load(Relaxed), dl.progress.total.load(Relaxed));
        let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        ui.add(egui::ProgressBar::new(frac).desired_width(360.0).text(tf!(
            "İndiriliyor ve doğrulanıyor… {:.1} MB",
            "Downloading and verifying… {:.1} MB",
            done as f64 / 1_048_576.0
        )));
        return;
    }
    if app.update_ready.is_some() {
        ui.label(RichText::new(t("Doğrulandı, kurulum başlatılıyor…", "Verified, starting the installer…")).color(ACCENT));
        return;
    }
    let busy = app.scan.is_some() || app.clean_job.is_some();
    ui.horizontal(|ui| {
        if app.update_confirm {
            ui.label(RichText::new(t(
                "Kurulum dosyası indirilip doğrulanacak, sonra Draxbyte kapanıp kurulum açılacak. Devam?",
                "The installer is downloaded and verified, then Draxbyte closes and the setup opens. Continue?",
            )).color(super::WARN));
            if ui.button(t("Evet, güncelle", "Yes, update")).clicked() {
                app.update_confirm = false;
                let ctx = ui.ctx().clone();
                app.start_update_download(&ctx);
            }
            if ui.button(t("Vazgeç", "Cancel")).clicked() {
                app.update_confirm = false;
            }
        } else if ui
            .add_enabled(!busy, egui::Button::new(t("Şimdi güncelle", "Update now")))
            .on_disabled_hover_text(t("Tarama/temizlik bitince güncelle", "Update after the scan/cleanup finishes"))
            .clicked()
        {
            app.update_confirm = true;
        }
    });
    if let Some(e) = &app.update_dl_error {
        ui.label(RichText::new(tf!("Güncelleme başarısız: {e}", "Update failed: {e}")).color(super::DANGER));
    }
}

pub fn about(app: &mut App, ui: &mut egui::Ui) {
    heading(ui, t("Hakkında", "About"), "");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&app.logo).fit_to_exact_size(egui::vec2(88.0, 88.0)));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("Drax").font(super::bold(30.0)));
                        ui.label(RichText::new("byte").font(super::bold(30.0)).color(ACCENT));
                    });
                    ui.label(RichText::new("CLEANER SYSTEM. MORE SPACE.").color(MUTED).small());
                    ui.label(RichText::new(tf!("Sürüm {}", "Version {}", env!("CARGO_PKG_VERSION"))).color(ACCENT));
                });
            });
            ui.add_space(8.0);
            ui.label(t("Yerel öncelikli disk tarama ve temizleme aracı. Tarama ve kural motoru tamamen çevrimdışı çalışır; yapay zekâ isteğe bağlıdır, varsayılan kapalıdır ve her istek senin onayınla gider.", "Local-first disk scanner and cleaner. Scanning and the rule engine work fully offline; AI is optional, off by default, and every request goes out only with your approval."));
        });

        ui.add_space(10.0);
        card(ui, |ui| {
            use crate::app::UpdateState;
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("Güncellemeler", "Updates")).font(super::bold(16.0)));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let busy = app.update_job.is_some();
                let label = if busy {
                    t("Kontrol ediliyor…", "Checking…")
                } else {
                    t("Güncellemeleri kontrol et", "Check for updates")
                };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    let ctx = ui.ctx().clone();
                    app.start_update_check(&ctx);
                }
                match &app.update {
                    Some(UpdateState::UpToDate) => {
                        ui.label(RichText::new(t("Güncelsin.", "You are up to date.")).color(ACCENT));
                    }
                    Some(UpdateState::Available(rel)) => {
                        ui.label(RichText::new(tf!("Yeni sürüm: {}", "New version: {}", rel.tag)).color(ACCENT));
                        ui.hyperlink_to(t("İndirme sayfası", "Download page"), &rel.url);
                    }
                    Some(UpdateState::Failed(e)) => {
                        ui.label(RichText::new(tf!("Kontrol edilemedi: {e}", "Could not check: {e}")).color(super::WARN));
                    }
                    None => {}
                }
            });
            let can_install = matches!(&app.update, Some(UpdateState::Available(r)) if r.setup_url.is_some());
            if cfg!(windows) && can_install {
                ui.add_space(6.0);
                update_install_row(app, ui);
            }
            ui.label(
                RichText::new(t(
                    "Kontrol yalnızca GitHub'dan en son sürüm numarasını sorar (tek istek). İndirme ve kurulum yalnızca sen \"Şimdi güncelle\" dersen başlar; dosya SHA256 ve imza sertifikası doğrulanmadan çalıştırılmaz. Düğmeye basmazsan ve Ayarlar'da açmazsan ağ isteği atılmaz.",
                    "Checking only asks GitHub for the latest version number (one request). Downloading and installing start only when you click “Update now”; the file is not run before its SHA256 and signing certificate are verified. No network request is made unless you press a button or enable the check in Settings.",
                ))
                .color(MUTED)
                .small(),
            );
        });

        ui.add_space(10.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("Geliştirici", "Developer")).font(super::bold(16.0)));
            ui.add_space(4.0);
            row(ui, t("Ad", "Name"), DEV_NAME);
            row(ui, t("Rol", "Role"), t("Yazılım geliştirici", "Software developer"));
            ui.horizontal(|ui| {
                label_cell(ui, t("Web sitesi", "Website"));
                ui.hyperlink_to(DEV_SITE.trim_start_matches("https://"), DEV_SITE);
            });
            ui.horizontal(|ui| {
                label_cell(ui, "GitHub");
                ui.hyperlink_to(DEV_GITHUB.trim_start_matches("https://"), DEV_GITHUB);
            });
            ui.horizontal(|ui| {
                label_cell(ui, t("Kaynak kod", "Source code"));
                ui.hyperlink_to(REPO.trim_start_matches("https://"), REPO);
            });
            row(ui, t("Lisans", "License"), "MIT");
        });

        ui.add_space(10.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("Bu bilgisayarda", "On this computer")).font(super::bold(16.0)));
            ui.add_space(4.0);
            row(ui, t("Ayar dosyası", "Config file"), app.cfg_path.display().to_string());
            row(ui, t("Veri klasörü", "Data folder"), data_dir().display().to_string());
            row(ui, t("Derleme", "Build"), if cfg!(debug_assertions) { "debug" } else { "release" });
            row(ui, "Platform", format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(t("Veri klasörünü aç", "Open data folder")).clicked() {
                    let dir = data_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    open_folder(&dir);
                }
                if ui.button(t("Draxbyte'ı kaldır…", "Uninstall Draxbyte…")).clicked() {
                    // the wizard is this same program in uninstall mode
                    if let Ok(exe) = std::env::current_exe() {
                        if std::process::Command::new(exe).arg("--uninstall").spawn().is_ok() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            });
        });

        ui.add_space(10.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("Açık kaynak bileşenler", "Open-source components")).font(super::bold(16.0)));
            ui.add_space(4.0);
            ui.label(t("Inter yazı tipi — © The Inter Project Authors, SIL Open Font License 1.1", "Inter typeface — © The Inter Project Authors, SIL Open Font License 1.1"));
            ui.label(t("egui / eframe — MIT veya Apache-2.0", "egui / eframe — MIT or Apache-2.0"));
            ui.label(t("rusqlite (SQLite), walkdir, rayon, tokio, reqwest, trash ve diğer Rust kütüphaneleri — MIT / Apache-2.0", "rusqlite (SQLite), walkdir, rayon, tokio, reqwest, trash and other Rust crates — MIT / Apache-2.0"));
        });

        ui.add_space(14.0);
        ui.label(RichText::new(tf!("© {COPYRIGHT_YEAR} {DEV_NAME}. Tüm hakları saklıdır.", "© {COPYRIGHT_YEAR} {DEV_NAME}. All rights reserved.")).color(MUTED).small());
    });
}
