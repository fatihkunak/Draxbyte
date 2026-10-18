//! Programs screen: every installed program with size and last use, and the removal dialog
//! (vendor uninstaller → leftovers → report). The heavy lifting is in `panels.rs` / `core::apps`.

use super::widgets::{card, danger_button, heading, primary_button};
use super::{ACCENT, MUTED, WARN};
use crate::app::App;
use crate::panels::{AppSort, FlowStage};
use draxbyte::core::apps::{self, human_days, Confidence, Removability};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, now_secs};
use draxbyte::tf;
use egui::{Align, Layout, RichText};
use egui_extras::{Column, TableBuilder};

pub fn programs(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Programlar", "Programs"),
        t(
            "Kurulu programları boyutu ve son kullanımıyla gör; istediğini artıklarıyla (AppData, kayıt defteri, servisler) birlikte tamamen kaldır.",
            "See installed programs with size and last use; remove any of them completely, including leftovers (AppData, registry, services).",
        ),
    );
    if !apps::supported() {
        ui.label(RichText::new(t("Bu platformda program yönetimi yok.", "Program management is not available on this platform.")).color(WARN));
        return;
    }
    let ctx = ui.ctx().clone();
    if !app.apps.loaded && app.apps.job.is_none() {
        app.apps_refresh(&ctx);
    }

    elevation_banner(app, ui);

    // DRAXBYTE_REMOVE=<name> opens that program's removal dialog once the list is loaded
    // (screenshots, demos).
    if app.apps.loaded && !app.apps.auto_opened {
        app.apps.auto_opened = true;
        if let Ok(q) = std::env::var("DRAXBYTE_REMOVE") {
            let q = q.to_lowercase();
            if let Some(i) = app.apps.list.iter().position(|a| a.name.to_lowercase().contains(&q)) {
                app.flow_open(&ctx, i);
            }
        }
    }

    // ---- toolbar
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.apps.filter).hint_text(t("Ara…", "Search…")).desired_width(200.0));
        ui.checkbox(&mut app.apps.only_unused, t("Kullanılmayanlar", "Unused"));
        ui.add_enabled(
            app.apps.only_unused,
            egui::DragValue::new(&mut app.apps.unused_days).range(7..=3650).suffix(t(" gün+", " days+")),
        );
        ui.checkbox(&mut app.apps.show_hidden, t("Sistem bileşenleri", "System components"))
            .on_hover_text(t(
                "Kurucusunun \"Uygulamalar ve özellikler\"den gizlediği çalışma zamanları (WebView2 gibi)",
                "Runtimes the installer hides from \"Apps & features\" (such as WebView2)",
            ));
        ui.separator();
        for (label, key) in [(t("Boyut", "Size"), AppSort::Size), (t("Son kullanım", "Last used"), AppSort::LastUsed), (t("Ad", "Name"), AppSort::Name)] {
            if ui.selectable_label(app.apps.sort == key, label).clicked() {
                app.apps.sort = key;
            }
        }
        let busy = app.apps.job.is_some();
        if ui.add_enabled(!busy, egui::Button::new(if busy { t("Yükleniyor…", "Loading…") } else { t("Yenile", "Refresh") })).clicked() {
            app.apps.loaded = false;
        }
    });

    // ---- visible rows
    let now = now_secs();
    let q = app.apps.filter.to_lowercase();
    let (only_unused, days, hidden) = (app.apps.only_unused, app.apps.unused_days, app.apps.show_hidden);
    let mut visible: Vec<usize> = app
        .apps
        .list
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            (hidden || !a.system_component)
                && (q.is_empty() || a.name.to_lowercase().contains(&q) || a.publisher.to_lowercase().contains(&q))
                && (!only_unused || a.days_unused(now).map(|d| d >= days).unwrap_or(false))
        })
        .map(|(i, _)| i)
        .collect();
    match app.apps.sort {
        AppSort::Size => visible.sort_by(|&a, &b| app.apps.list[b].size_bytes.cmp(&app.apps.list[a].size_bytes)),
        AppSort::LastUsed => visible.sort_by_key(|&i| app.apps.list[i].last_used.unwrap_or(i64::MAX)),
        AppSort::Name => visible.sort_by_key(|&i| app.apps.list[i].name.to_lowercase()),
    }
    let total: u64 = visible.iter().map(|&i| app.apps.list[i].size_bytes).sum();
    let unused_n = app.apps.list.iter().filter(|a| a.days_unused(now).map(|d| d >= days).unwrap_or(false)).count();
    ui.label(
        RichText::new(tf!(
            "{} program · {} · {} gündür kullanılmayan: {}",
            "{} programs · {} · unused for {} days: {}",
            visible.len(),
            human_bytes(total),
            days,
            unused_n
        ))
        .color(MUTED),
    );
    ui.add_space(4.0);

    if app.apps.list.is_empty() {
        ui.label(t("Liste yükleniyor…", "Loading the list…"));
        return;
    }

    let mut open: Option<usize> = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(240.0).clip(true))
        .column(Column::exact(90.0))
        .column(Column::exact(120.0))
        .column(Column::exact(110.0))
        .column(Column::exact(130.0))
        .header(24.0, |mut h| {
            for label in [t("Program", "Program"), t("Boyut", "Size"), t("Son kullanım", "Last used"), t("Kurulum", "Installed"), ""] {
                h.col(|ui| {
                    ui.label(RichText::new(label).strong());
                });
            }
        })
        .body(|body| {
            body.rows(44.0, visible.len(), |mut row| {
                let idx = visible[row.index()];
                let a = &app.apps.list[idx];
                row.col(|ui| {
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(&a.name).strong()).truncate());
                        let sub = format!("{}{}{}", a.publisher, if a.publisher.is_empty() || a.version.is_empty() { "" } else { " · " }, a.version);
                        ui.add(egui::Label::new(RichText::new(sub).color(MUTED).small()).truncate());
                    });
                });
                row.col(|ui| {
                    ui.label(if a.size_bytes > 0 { human_bytes(a.size_bytes) } else { "—".into() });
                });
                row.col(|ui| match a.days_unused(now) {
                    Some(d) => {
                        let txt = RichText::new(human_days(d));
                        ui.label(if d >= days { txt.color(WARN) } else { txt });
                    }
                    None => {
                        ui.label(RichText::new(t("bilinmiyor", "unknown")).color(MUTED))
                            .on_hover_text(t(
                                "Windows bu program için kullanım kaydı tutmuyor (servis, komut satırı aracı ya da arka planda çalışıyor olabilir).",
                                "The system keeps no usage record for this program (it may be a service, a command-line tool or run in the background).",
                            ));
                    }
                });
                row.col(|ui| {
                    ui.label(
                        a.install_date
                            .map(|d| human_days(((now - d) / 86_400).max(0)))
                            .unwrap_or_else(|| "—".into()),
                    );
                });
                row.col(|ui| match &a.removable {
                    Removability::Sealed(why) => {
                        if ui.button(t("Kaldırılamaz…", "Can't remove…")).on_hover_text(why).clicked() {
                            open = Some(idx);
                        }
                    }
                    Removability::Blocked(why) => {
                        if ui.add(danger_button(t("Zorla kaldır", "Force remove"))).on_hover_text(why).clicked() {
                            open = Some(idx);
                        }
                    }
                    Removability::Normal => {
                        if ui.button(t("Kaldır…", "Remove…")).clicked() {
                            open = Some(idx);
                        }
                    }
                });
            });
        });
    if let Some(i) = open {
        app.flow_open(&ctx, i);
    }
}

fn elevation_banner(app: &mut App, ui: &mut egui::Ui) {
    // macOS asks for the password at the moment an action needs it; only Windows needs a restart.
    if !cfg!(windows) || draxbyte::core::elevate::is_elevated() {
        return;
    }
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(t(
                    "Yönetici değilsin: tüm kullanıcılar için kurulu programlar, servisler ve sistem klasörleri yönetici izni ister.",
                    "Not running as administrator: programs installed for all users, services and system folders need it.",
                ))
                .color(WARN),
            );
            if ui.button(t("Yönetici olarak yeniden aç", "Restart as administrator")).clicked() {
                match draxbyte::core::elevate::relaunch_elevated() {
                    Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => app.toast(tf!("Yönetici izni alınamadı: {e}", "Could not get administrator rights: {e}")),
                }
            }
        });
    });
    ui.add_space(8.0);
}

/// The removal dialog; drawn on top of every screen while a removal is open.
pub fn flow_modal(app: &mut App, ctx: &egui::Context) {
    if app.apps.flow.is_none() {
        return;
    }
    let mut action = FlowAction::None;
    #[allow(deprecated)] // needs one stable id across frames; Id::unique would change every frame
    let id = egui::Id::new("remove_flow");
    let modal = egui::Modal::new(id).show(ctx, |ui| {
        ui.set_width(660.0);
        let f = app.apps.flow.as_mut().unwrap();
        ui.label(RichText::new(tf!("Kaldır: {}", "Remove: {}", f.app.name)).font(super::bold(20.0)));
        let sub = format!("{} {}", f.app.publisher, f.app.version);
        ui.label(RichText::new(sub.trim()).color(MUTED));
        ui.add_space(8.0);

        match f.stage {
            FlowStage::Review => review(f, ui, &mut action),
            FlowStage::Working => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Çalışıyor — bu pencereyi kapatma…", "Working — keep this window open…"));
                });
                for l in &f.log {
                    ui.label(RichText::new(l).color(MUTED).small());
                }
            }
            FlowStage::Cleanup => cleanup(f, ui, &mut action),
            FlowStage::Done => done(f, ui, &mut action, app_backup(&app.apps.last_backup)),
        }
    });
    match action {
        FlowAction::Run => app.flow_run(ctx),
        FlowAction::Clean => app.flow_clean(ctx),
        FlowAction::CleanSealed => {
            if let Some(f) = app.apps.flow.as_mut() {
                f.items = f.preview.clone().unwrap_or_default().into_iter().map(|l| (l, true)).collect();
                f.log = vec![t("Bu uygulama kaldırılamaz; yalnızca önbelleği ve geçici verisi temizlenir.", "This app cannot be removed; only its caches and temporary data are cleaned.").to_string()];
                f.stage = FlowStage::Cleanup;
            }
        }
        FlowAction::Close => app.flow_close(),
        FlowAction::Restore => {
            if let Some(dir) = app.apps.last_backup.clone() {
                let msg = match apps::restore_backup(&dir) {
                    Ok(n) => tf!("{} kayıt defteri yedeği geri yüklendi", "{} registry backups restored", n),
                    Err(e) => tf!("Geri yüklenemedi: {e}", "Could not restore: {e}"),
                };
                if let Some(f) = app.apps.flow.as_mut() {
                    f.restore_msg = Some(msg);
                }
            }
        }
        FlowAction::None => {
            let working = app.apps.flow.as_ref().map(|f| f.stage == FlowStage::Working).unwrap_or(false);
            if modal.should_close() && !working {
                app.flow_close();
            }
        }
    }
    if app.apps.flow.as_ref().map(|f| f.stage == FlowStage::Working).unwrap_or(false) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

fn app_backup(p: &Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    p.clone()
}

enum FlowAction {
    None,
    Run,
    Clean,
    CleanSealed,
    Close,
    Restore,
}

fn review(f: &mut crate::panels::Flow, ui: &mut egui::Ui, action: &mut FlowAction) {
    let mut sealed = false;
    if let Removability::Sealed(why) = &f.app.removable {
        ui.label(RichText::new(why).color(WARN));
        sealed = true;
    }
    if let Some(loc) = &f.app.install_location {
        ui.label(RichText::new(loc.display().to_string()).color(MUTED).small());
    }
    if f.app.name == "Microsoft Edge" {
        ui.add_space(4.0);
        ui.label(
            RichText::new(t(
                "Edge'i kaldırmak Windows'un onayladığı bir işlem değildir. Windows Update geri getirmesin diye engel konur. \
                 Edge'e bağlı Windows özellikleri (örn. bazı arama/pano bağlantıları) etkilenebilir; WebView2 çalışma zamanı ayrı bir programdır ve burada KALDIRILMAZ.",
                "Removing Edge is not something Windows supports. A block is added so Windows Update does not put it back. \
                 Windows features that use Edge (some search/link handlers) may be affected; the WebView2 runtime is a separate program and is NOT removed here.",
            ))
            .color(WARN)
            .small(),
        );
    }
    if !f.running.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(tf!("Şu an çalışıyor: {}", "Running now: {}", f.running.join(", "))).color(WARN));
        ui.checkbox(&mut f.close_running, t("Kaldırmadan önce kapat (kaydedilmemiş işler kaybolabilir)", "Close it first (unsaved work may be lost)"));
    }
    ui.add_space(6.0);
    if f.app.uninstall_cmd.is_some() {
        ui.radio_value(&mut f.force, false, t("Üreticinin kaldırıcısını çalıştır, sonra artıkları temizle (önerilir)", "Run the vendor's uninstaller, then clean the leftovers (recommended)"));
        if f.app.quiet_uninstall_cmd.is_some() {
            ui.add_enabled_ui(!f.force, |ui| {
                ui.indent("silent", |ui| ui.checkbox(&mut f.silent, t("Sessiz kaldır (soru sormadan)", "Silent (no questions asked)")));
            });
        }
    }
    ui.radio_value(&mut f.force, true, t("Zorla kaldır: kaldırıcıyı atla, her şeyi doğrudan sil", "Force remove: skip the uninstaller, delete everything directly"));

    ui.add_space(6.0);
    ui.label(RichText::new(t("Bulunanlar (kaldırıcıdan sonra yeniden taranır):", "Found (rescanned after the uninstaller):")).strong());
    match &f.preview {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t("Taranıyor…", "Scanning…"));
            });
        }
        Some(items) if items.is_empty() => {
            ui.label(RichText::new(t("Ek bir artık bulunmadı.", "No extra leftovers found.")).color(MUTED));
        }
        Some(items) => {
            let total: u64 = items.iter().map(|l| l.size).sum();
            ui.label(RichText::new(tf!("{} öğe · {}", "{} items · {}", items.len(), human_bytes(total))).color(MUTED));
            egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                for l in items {
                    let mark = if l.confidence == Confidence::Medium { "?  " } else { "" };
                    let size = if l.size > 0 { format!("{}  ", human_bytes(l.size)) } else { String::new() };
                    let col = if l.confidence == Confidence::Medium { WARN } else { ui.visuals().text_color() };
                    ui.add(egui::Label::new(RichText::new(format!("{mark}{size}{}", l.label)).small().color(col)).truncate());
                }
            });
        }
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if sealed {
            let has = f.preview.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
            if ui.add_enabled(has, primary_button(t("Önbelleğini ve geçici verisini temizle", "Clean its caches and temporary data"))).clicked() {
                *action = FlowAction::CleanSealed;
            }
        } else if ui.add_enabled(f.preview.is_some(), danger_button(t("Kaldır", "Remove"))).clicked() {
            *action = FlowAction::Run;
        }
        if ui.button(t("Vazgeç", "Cancel")).clicked() {
            *action = FlowAction::Close;
        }
    });
}

fn cleanup(f: &mut crate::panels::Flow, ui: &mut egui::Ui, action: &mut FlowAction) {
    for l in &f.log {
        ui.label(RichText::new(l).color(MUTED).small());
    }
    ui.add_space(4.0);
    if f.items.is_empty() {
        ui.label(RichText::new(t("Program kaldırıldı; temizlenecek artık kalmadı.", "The program is removed; nothing is left to clean.")).color(ACCENT));
        if ui.button(t("Bitir", "Finish")).clicked() {
            *action = FlowAction::Close;
        }
        return;
    }
    ui.label(RichText::new(t("Kalanlar — silinecekleri işaretle:", "What is left — tick what to delete:")).strong());
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        for (l, on) in f.items.iter_mut() {
            ui.horizontal(|ui| {
                ui.checkbox(on, "");
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(&l.label).small()).truncate());
                    let mut meta = if l.size > 0 { format!("{} · ", human_bytes(l.size)) } else { String::new() };
                    meta.push_str(&l.why);
                    let col = if l.confidence == Confidence::Medium { WARN } else { MUTED };
                    ui.add(egui::Label::new(RichText::new(meta).color(col).small()).truncate());
                });
            });
        }
    });
    if f.confirm_probable {
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.label(RichText::new(t("Bunlar yalnızca OLASI eşleşmeler; başka programlara ya da kendi verilerine ait olabilir:", "These are only PROBABLE matches; they may belong to other programs or hold your own data:")).color(WARN).strong());
            for l in f.probable_selected() {
                ui.add(egui::Label::new(RichText::new(format!("{} — {}", l.label, l.why)).small()).wrap());
            }
            ui.horizontal(|ui| {
                if ui.add(danger_button(t("Evet, bunları da sil", "Yes, delete these too"))).clicked() {
                    *action = FlowAction::Clean;
                }
                if ui.button(t("Geri dön", "Go back")).clicked() {
                    f.confirm_probable = false;
                }
            });
        });
    }
    ui.add_space(6.0);
    ui.checkbox(&mut f.permanent, t("Kalıcı sil (Çöp Kutusu'nu atla, disk hemen boşalır)", "Delete for good (skip the Recycle Bin, frees the disk at once)"));
    ui.label(RichText::new(t("Kayıt defteri anahtarları silinmeden önce yedeklenir; sonradan geri yüklenebilir.", "Registry keys are exported before deletion and can be restored later.")).color(MUTED).small());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let n = f.items.iter().filter(|(_, on)| *on).count();
        if f.confirm_probable {
            // second, explicit confirmation for items that are only probable matches
        } else if ui.add_enabled(n > 0, primary_button(&tf!("Seçilenleri sil ({}, {})", "Delete selected ({}, {})", n, human_bytes(f.selected_bytes())))).clicked() {
            if f.probable_selected().is_empty() {
                *action = FlowAction::Clean;
            } else {
                f.confirm_probable = true;
            }
        }
        if ui.button(t("Hiçbirini silme", "Delete nothing")).clicked() {
            *action = FlowAction::Close;
        }
    });
}

fn done(f: &mut crate::panels::Flow, ui: &mut egui::Ui, action: &mut FlowAction, backup: Option<std::path::PathBuf>) {
    let Some(rep) = &f.report else { return };
    ui.label(
        RichText::new(tf!("Bitti · {} geri kazanıldı", "Done · {} freed", human_bytes(rep.freed)))
            .color(ACCENT)
            .font(super::bold(16.0)),
    );
    if let Some(n) = &f.after_note {
        ui.label(RichText::new(n).color(MUTED));
    }
    egui::CollapsingHeader::new(RichText::new(tf!("Silinenler ({})", "Removed ({})", rep.items.iter().filter(|i| i.ok).count())).small())
        .show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                for i in rep.items.iter().filter(|i| i.ok) {
                    ui.add(egui::Label::new(RichText::new(&i.label).small().color(MUTED)).truncate());
                }
            });
        });
    let failed: Vec<_> = rep.items.iter().filter(|i| !i.ok).collect();
    if !failed.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(tf!("{} öğe silinemedi:", "{} items could not be removed:", failed.len())).color(WARN));
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            for i in failed {
                ui.add(egui::Label::new(RichText::new(format!("{} — {}", i.label, i.message)).small()).wrap());
            }
        });
    }
    if let Some(dir) = backup {
        ui.label(RichText::new(tf!("Yedek: {}", "Backup: {}", dir.display())).color(MUTED).small());
        ui.horizontal(|ui| {
            if cfg!(windows) && ui.button(t("Kayıt defterini geri yükle", "Restore the registry")).clicked() {
                *action = FlowAction::Restore;
            }
            if ui.button(t("Yedek klasörünü aç", "Open backup folder")).clicked() {
                #[cfg(windows)]
                let opener = "explorer";
                #[cfg(target_os = "macos")]
                let opener = "open";
                #[cfg(not(any(windows, target_os = "macos")))]
                let opener = "xdg-open";
                let _ = std::process::Command::new(opener).arg(&dir).spawn();
            }
        });
    }
    if let Some(m) = &f.restore_msg {
        ui.label(RichText::new(m).color(ACCENT).small());
    }
    ui.add_space(8.0);
    if ui.button(t("Kapat", "Close")).clicked() {
        *action = FlowAction::Close;
    }
}
