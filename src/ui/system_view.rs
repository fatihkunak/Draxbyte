//! Memory & startup screen: what uses RAM, closing programs you pick, the (temporary) memory
//! trim, and the programs that start with Windows.

use super::widgets::{card, danger_button, heading};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::App;
use draxbyte::core::startup::Scope;
use draxbyte::i18n::t;
use draxbyte::models::human_bytes;
use draxbyte::tf;
use egui::{Align, Layout, RichText};
use egui_extras::{Column, TableBuilder};

pub fn system(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Bellek ve Başlangıç", "Memory & Startup"),
        t(
            "Belleği neyin yediğini gör, seçtiğin programı kapat, Windows'la birlikte açılanları yönet.",
            "See what uses memory, close the programs you pick, manage what starts with Windows.",
        ),
    );
    let ctx = ui.ctx().clone();
    let stale = app.sys.last.map(|l| l.elapsed().as_secs() >= 4).unwrap_or(true);
    if stale && app.sys.job.is_none() {
        app.sys_refresh(&ctx);
    }
    if !app.sys.startup_loaded {
        app.startup_reload();
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(2));

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        memory_card(app, ui, &ctx);
        ui.add_space(12.0);
        processes_card(app, ui);
        ui.add_space(12.0);
        startup_card(app, ui);
    });
}

fn bar(ui: &mut egui::Ui, used: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 7.0, super::BORDER);
    let mut fill = rect;
    fill.set_width(rect.width() * used.clamp(0.0, 1.0));
    p.rect_filled(fill, 7.0, color);
}

fn memory_card(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("Bellek (RAM)", "Memory (RAM)")).font(super::bold(17.0)));
        match app.sys.mem {
            None => {
                ui.label(RichText::new(t("Okunuyor…", "Reading…")).color(MUTED));
            }
            Some(m) => {
                let frac = if m.total > 0 { m.used() as f32 / m.total as f32 } else { 0.0 };
                bar(ui, frac, if frac > 0.85 { DANGER } else if frac > 0.7 { WARN } else { ACCENT });
                ui.label(tf!(
                    "{} kullanımda / {} toplam · {} boş (kullanılabilir)",
                    "{} in use / {} total · {} available",
                    human_bytes(m.used()),
                    human_bytes(m.total),
                    human_bytes(m.available)
                ));
                if m.swap_total > 0 {
                    ui.label(
                        RichText::new(tf!("Sayfa dosyası: {} / {}", "Page file: {} / {}", human_bytes(m.swap_used), human_bytes(m.swap_total)))
                            .color(MUTED)
                            .small(),
                    );
                }
            }
        }
        ui.add_space(6.0);
        if cfg!(windows) {
            ui.horizontal(|ui| {
                let busy = app.sys.trim_job.is_some();
                if ui.add_enabled(!busy, egui::Button::new(if busy { t("Boşaltılıyor…", "Freeing…") } else { t("Belleği boşalt", "Free memory") })).clicked() {
                    app.sys_trim(ctx);
                }
                match &app.sys.trim_result {
                    Some(Ok(r)) => {
                        ui.label(
                            RichText::new(tf!(
                                "{} işlem · boş bellek {} → {}",
                                "{} processes · available {} → {}",
                                r.trimmed,
                                human_bytes(r.available_before),
                                human_bytes(r.available_after)
                            ))
                            .color(ACCENT),
                        );
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(WARN));
                    }
                    None => {}
                }
            });
            ui.label(
                RichText::new(t(
                    "Dürüst not: bu, programların boştaki belleğini diske (sayfa dosyasına) alır. Boş bellek hemen artar ama bir program o sayfalara tekrar ihtiyaç duyunca diskten geri okur; kazanç geçicidir. Kalıcı çözüm: aşağıdan çok bellek yiyen programı kapatmak ve başlangıçtakileri azaltmak.",
                    "Honest note: this moves the idle memory of programs out to the page file. Free memory rises at once, but a program that needs those pages again reads them back from disk, so the gain is temporary. The lasting fix is to close the programs that use a lot, below, and to cut what starts at logon.",
                ))
                .color(MUTED)
                .small(),
            );
        }
    });
}

fn processes_card(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("En çok bellek kullananlar", "Top memory users")).font(super::bold(17.0)));
            ui.add(egui::TextEdit::singleline(&mut app.sys.filter).hint_text(t("Ara…", "Search…")).desired_width(160.0));
        });
        if let Some(m) = &app.sys.msg {
            ui.label(RichText::new(m).color(WARN));
        }
        let q = app.sys.filter.to_lowercase();
        let rows: Vec<usize> = app
            .sys
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| q.is_empty() || g.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .take(40)
            .collect();
        let mut close: Option<(usize, bool)> = None;
        let mut ask: Option<(usize, bool)> = None;
        let mut protect: Option<(String, bool)> = None;
        let confirm = app.sys.confirm_close;
        let user_protected: Vec<bool> = app.sys.groups.iter().map(|g| app.is_user_protected(&g.name)).collect();
        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(180.0).clip(true))
            .column(Column::exact(60.0))
            .column(Column::exact(90.0))
            .column(Column::exact(380.0))
            .vscroll(false)
            .header(22.0, |mut h| {
                for l in [t("Program", "Program"), t("İşlem", "Procs"), t("Bellek", "Memory"), ""] {
                    h.col(|ui| {
                        ui.label(RichText::new(l).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(28.0, rows.len(), |mut row| {
                    let idx = rows[row.index()];
                    let g = &app.sys.groups[idx];
                    row.col(|ui| {
                        let r = ui.add(egui::Label::new(RichText::new(&g.name)).truncate());
                        if let Some(e) = &g.exe {
                            r.on_hover_text(e.display().to_string());
                        }
                    });
                    row.col(|ui| {
                        ui.label(g.pids.len().to_string());
                    });
                    row.col(|ui| {
                        ui.label(human_bytes(g.bytes));
                    });
                    row.col(|ui| {
                        if g.protected {
                            ui.label(RichText::new(t("sistem — kapatılamaz", "system — cannot be closed")).color(MUTED).small());
                        } else if user_protected[idx] {
                            ui.label(RichText::new(t("korumalı (senin listen)", "protected (your list)")).color(ACCENT).small());
                            if ui.small_button(t("Korumayı kaldır", "Unprotect")).clicked() {
                                protect = Some((g.name.clone(), false));
                            }
                        } else if confirm.map(|(i, _)| i) == Some(idx) {
                            let force = confirm.map(|(_, f)| f).unwrap_or(false);
                            ui.label(RichText::new(if force { t("Kaydedilmemiş iş kaybolur. Emin misin?", "Unsaved work is lost. Sure?") } else { t("Kapatılsın mı?", "Close it?") }).color(WARN).small());
                            if ui.add(danger_button(t("Evet", "Yes"))).clicked() {
                                close = Some((idx, force));
                            }
                            if ui.button(t("Vazgeç", "No")).clicked() {
                                ask = Some((usize::MAX, false));
                            }
                        } else {
                            if ui.button(t("Kapat", "Close")).clicked() {
                                ask = Some((idx, false));
                            }
                            if ui.button(t("Zorla kapat", "Force")).clicked() {
                                ask = Some((idx, true));
                            }
                            if ui.small_button(t("Koru", "Protect")).on_hover_text(t("Koru: bu program Draxbyte'tan asla kapatılamasın", "Protect: never allow Draxbyte to close this program")).clicked() {
                                protect = Some((g.name.clone(), true));
                            }
                        }
                    });
                });
            });
        if let Some((name, on)) = protect {
            app.set_user_protected(&name, on);
        }
        if let Some((i, f)) = ask {
            app.sys.confirm_close = if i == usize::MAX { None } else { Some((i, f)) };
        }
        if let Some((i, f)) = close {
            app.sys.confirm_close = None;
            app.sys_close(i, f);
        }
    });
}

fn startup_card(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("Windows'la birlikte açılanlar", "Starts with Windows")).font(super::bold(17.0)));
        if !cfg!(windows) {
            ui.label(RichText::new(t("Bu liste yalnızca Windows'ta.", "This list is Windows-only.")).color(MUTED));
            return;
        }
        ui.label(
            RichText::new(t(
                "Kapatmak silmez; Görev Yöneticisi'ndeki anahtarı çevirir, istediğin zaman geri açarsın.",
                "Turning one off deletes nothing; it flips the same switch Task Manager uses, and you can turn it back on.",
            ))
            .color(MUTED)
            .small(),
        );
        let mut toggle: Option<(usize, bool)> = None;
        for (i, e) in app.sys.startup.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut on = e.enabled;
                if ui.checkbox(&mut on, "").changed() {
                    toggle = Some((i, on));
                }
                ui.vertical(|ui| {
                    let scope = match &e.source {
                        draxbyte::core::startup::Source::RunKey(Scope::Machine, _) | draxbyte::core::startup::Source::Folder(Scope::Machine) => t(" · tüm kullanıcılar", " · all users"),
                        _ => "",
                    };
                    ui.add(egui::Label::new(RichText::new(format!("{}{}", e.name, scope)).strong()).truncate());
                    ui.add(egui::Label::new(RichText::new(&e.command).color(MUTED).small()).truncate());
                });
            });
        }
        if app.sys.startup.is_empty() {
            ui.label(RichText::new(t("Başlangıçta açılan program yok.", "Nothing starts at logon.")).color(MUTED));
        }
        if let Some((i, on)) = toggle {
            app.startup_toggle(i, on);
        }
    });
}
