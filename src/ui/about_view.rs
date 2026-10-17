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
            if ui.button(t("Veri klasörünü aç", "Open data folder")).clicked() {
                let dir = data_dir();
                let _ = std::fs::create_dir_all(&dir);
                open_folder(&dir);
            }
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
