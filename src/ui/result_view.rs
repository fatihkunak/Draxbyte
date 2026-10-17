//! Results table, the confirmation/AI modals and the cleanup screen.

use super::widgets::{card, category_color, danger_button, heading, primary_button, stat};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::{AiNote, App, Confirm, SortKey, View};
use draxbyte::core::cleaner::CleanOptions;
use draxbyte::models::{human_bytes, now_secs, Category, Outcome};
use egui::{Align, Layout, RichText};
use egui_extras::{Column, TableBuilder};
use std::sync::atomic::Ordering::Relaxed;

enum Act {
    Toggle(usize),
    Ask(usize),
}

pub fn results(app: &mut App, ui: &mut egui::Ui) {
    if app.view_dirty {
        app.refresh_view();
    }
    heading(
        ui,
        "Sonuçlar",
        "Kutuları işaretle; hiçbir şey onay ekranından önce silinmez.",
    );

    ui.horizontal_wrapped(|ui| {
        for (label, cat) in [
            ("Tümü", None),
            ("Güvenli", Some(Category::Green)),
            ("Dikkat", Some(Category::Yellow)),
            ("Silme", Some(Category::Red)),
        ] {
            if ui.selectable_label(app.filter_cat == cat, label).clicked() {
                app.filter_cat = cat;
                app.view_dirty = true;
            }
        }
        if ui
            .add(
                egui::TextEdit::singleline(&mut app.filter_text)
                    .hint_text("Yolda ara")
                    .desired_width(180.0),
            )
            .changed()
        {
            app.view_dirty = true;
        }
        ui.separator();
        if ui.button("Tüm yeşilleri seç").clicked() {
            app.select_category(Category::Green);
        }
        if ui.button("Sarıları incele").clicked() {
            app.filter_cat = Some(Category::Yellow);
            app.view_dirty = true;
        }
        if ui.button("Seçimi temizle").clicked() {
            app.rows.iter_mut().for_each(|r| r.selected = false);
        }
    });
    ui.add_space(6.0);

    let (sel_count, sel_bytes) = app
        .selected()
        .fold((0usize, 0u64), |(n, b), r| (n + 1, b + r.item.size));
    egui::Panel::bottom("results_bar")
        .frame(egui::Frame::new())
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{sel_count} öğe seçili · {}",
                        human_bytes(sel_bytes)
                    ))
                    .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let busy = app.scan.is_some() || app.clean_job.is_some();
                    if ui
                        .add_enabled(sel_count > 0 && !busy, primary_button("Temizle…"))
                        .clicked()
                    {
                        app.confirm = Some(Confirm {
                            dry_run: false,
                            permanent: !app.cfg.cleanup.use_trash,
                            typed: String::new(),
                        });
                    }
                });
            });
        });

    if app.rows.is_empty() {
        ui.label("Gösterilecek sonuç yok. Ana sayfadan bir tarama başlat.");
        return;
    }

    let now = now_secs();
    let mut acts: Vec<Act> = Vec::new();
    let mut new_sort = None;
    let ai_on = app.ai.is_some();
    let ai_tip = app
        .ai_error
        .clone()
        .unwrap_or_else(|| "AI kapalı (Ayarlar → AI)".into());

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(28.0))
        .column(Column::exact(78.0))
        .column(Column::exact(86.0))
        .column(Column::exact(60.0))
        .column(Column::initial(230.0).at_least(120.0).clip(true))
        .column(Column::remainder().at_least(220.0).clip(true))
        .column(Column::initial(120.0).at_least(90.0).clip(true))
        .header(24.0, |mut h| {
            let mut sortable = |h: &mut egui_extras::TableRow,
                                label: &str,
                                key: SortKey,
                                cur: (SortKey, bool)| {
                h.col(|ui| {
                    let arrow = if cur.0 == key {
                        if cur.1 {
                            " ↑"
                        } else {
                            " ↓"
                        }
                    } else {
                        ""
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(format!("{label}{arrow}")).strong())
                                .frame(false),
                        )
                        .clicked()
                    {
                        new_sort = Some(key);
                    }
                });
            };
            let cur = app.sort;
            h.col(|_| {});
            sortable(&mut h, "Kategori", SortKey::Category, cur);
            sortable(&mut h, "Boyut", SortKey::Size, cur);
            sortable(&mut h, "Yaş", SortKey::Age, cur);
            h.col(|ui| {
                ui.label(RichText::new("Neden").strong());
            });
            sortable(&mut h, "Yol", SortKey::Path, cur);
            h.col(|ui| {
                ui.label(RichText::new("AI").strong());
            });
        })
        .body(|body| {
            body.rows(28.0, app.visible.len(), |mut row| {
                let idx = app.visible[row.index()];
                let r = &mut app.rows[idx];
                let color = category_color(r.item.category);
                let red = r.item.category == Category::Red;
                row.col(|ui| {
                    let mut sel = r.selected;
                    if ui
                        .add_enabled(!red, egui::Checkbox::without_text(&mut sel))
                        .changed()
                    {
                        acts.push(Act::Toggle(idx));
                    }
                });
                row.col(|ui| {
                    ui.label(RichText::new(format!("• {}", r.item.category.label())).color(color));
                });
                row.col(|ui| {
                    ui.label(human_bytes(r.item.size));
                });
                row.col(|ui| {
                    let days = r.item.age_days(now);
                    // epoch-zero mtimes (some apps reset them) would read "17000 days"
                    ui.label(if days > 20 * 365 {
                        "—".to_string()
                    } else {
                        format!("{days} g")
                    });
                });
                row.col(|ui| {
                    ui.label(RichText::new(&r.item.reason).color(MUTED).small())
                        .on_hover_text(&r.item.reason);
                });
                row.col(|ui| {
                    let p = r.item.path.to_string_lossy();
                    let text = if r.item.is_dir {
                        format!("{p}{}", std::path::MAIN_SEPARATOR)
                    } else {
                        p.to_string()
                    };
                    ui.add(egui::Label::new(text).truncate())
                        .on_hover_text(p.as_ref());
                });
                row.col(|ui| match &r.ai {
                    Some(AiNote { text, cached }) => {
                        let tag = if *cached { " (önbellek)" } else { "" };
                        ui.add(
                            egui::Label::new(RichText::new(format!("{text}{tag}")).small())
                                .truncate(),
                        )
                        .on_hover_text(text);
                    }
                    None => {
                        let hint = if r.item.unknown {
                            "Belirsiz öğe — AI'ya sor"
                        } else {
                            "AI'ya sor"
                        };
                        let btn = egui::Button::new(RichText::new("AI'ya Sor").small());
                        let resp = ui.add_enabled(ai_on, btn);
                        if ai_on {
                            if resp.on_hover_text(hint).clicked() {
                                acts.push(Act::Ask(idx));
                            }
                        } else {
                            resp.on_disabled_hover_text(&ai_tip);
                        }
                    }
                });
            });
        });

    if let Some(k) = new_sort {
        app.sort = if app.sort.0 == k {
            (k, !app.sort.1)
        } else {
            (k, k == SortKey::Path)
        };
        app.view_dirty = true;
    }
    for a in acts {
        match a {
            Act::Toggle(i) => app.rows[i].selected = !app.rows[i].selected,
            Act::Ask(i) => {
                let free = app
                    .ai
                    .as_ref()
                    .map(|s| s.is_cached(&app.rows[i].item))
                    .unwrap_or(false);
                if free {
                    let ctx = ui.ctx().clone();
                    app.ask_ai(&ctx, i);
                } else {
                    app.ai_confirm = Some(i);
                }
            }
        }
    }
}

pub fn confirm_modal(app: &mut App, ctx: &egui::Context) {
    if app.confirm.is_none() {
        return;
    }
    let (mut n, mut bytes, mut green_b, mut yellow_b, mut yellow_n) =
        (0usize, 0u64, 0u64, 0u64, 0usize);
    for r in app.selected() {
        n += 1;
        bytes += r.item.size;
        match r.item.category {
            Category::Green => green_b += r.item.size,
            _ => {
                yellow_b += r.item.size;
                yellow_n += 1;
            }
        }
    }
    let need_typed = app.cfg.cleanup.confirm_permanent_delete;
    let mut start: Option<CleanOptions> = None;
    let mut cancel = false;

    let modal = egui::Modal::new(egui::Id::unique("confirm_clean")).show(ctx, |ui| {
        ui.set_width(440.0);
        let c = app.confirm.as_mut().unwrap();
        ui.label(RichText::new("Temizliği onayla").size(20.0).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{n} öğe, {} silinecek", human_bytes(bytes))).size(16.0));
        ui.label(RichText::new(format!("Yeşil: {}", human_bytes(green_b))).color(ACCENT));
        if yellow_n > 0 {
            ui.label(
                RichText::new(format!(
                    "Dikkat (sarı): {yellow_n} öğe, {} — emin ol",
                    human_bytes(yellow_b)
                ))
                .color(WARN),
            );
        }
        ui.add_space(8.0);
        ui.checkbox(
            &mut c.dry_run,
            "Kuru çalıştırma (hiçbir şey silme, sadece raporla)",
        );
        if !c.dry_run {
            ui.radio_value(
                &mut c.permanent,
                false,
                "Çöp Kutusu'na taşı (geri alınabilir)",
            );
            ui.radio_value(
                &mut c.permanent,
                true,
                RichText::new("Kalıcı sil (geri alınamaz)").color(DANGER),
            );
            if c.permanent && need_typed {
                ui.label(RichText::new("Onaylamak için SIL yaz:").color(DANGER));
                ui.text_edit_singleline(&mut c.typed);
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Vazgeç").clicked() {
                cancel = true;
            }
            let permanent = !c.dry_run && c.permanent;
            let ok = !(permanent && need_typed && c.typed.trim() != "SIL");
            let btn = if permanent {
                danger_button("Kalıcı sil")
            } else {
                primary_button(if c.dry_run {
                    "Kuru çalıştır"
                } else {
                    "Çöpe taşı"
                })
            };
            if ui.add_enabled(ok, btn).clicked() {
                start = Some(CleanOptions {
                    dry_run: c.dry_run,
                    permanent,
                });
            }
        });
    });
    if modal.should_close() {
        cancel = true;
    }
    if let Some(opts) = start {
        app.confirm = None;
        app.start_clean(ctx, opts);
    } else if cancel {
        app.confirm = None;
    }
}

pub fn ai_modal(app: &mut App, ctx: &egui::Context) {
    let Some(idx) = app.ai_confirm else { return };
    let (Some(svc), Some(row)) = (app.ai.clone(), app.rows.get(idx)) else {
        app.ai_confirm = None;
        return;
    };
    let (prompt, est) = svc.preview(&row.item);
    let used = svc.tokens_today().unwrap_or(0);
    let limit = svc.daily_limit();
    let mut send = false;
    let mut cancel = false;
    let modal = egui::Modal::new(egui::Id::unique("ai_confirm")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(RichText::new("AI'ya sor — onay").size(20.0).strong());
        ui.label(
            RichText::new("Yalnızca aşağıdaki metin gönderilir (dosya içeriği değil):")
                .color(MUTED),
        );
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(super::SIDEBAR)
            .corner_radius(8)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new(&prompt.user).monospace().small());
            });
        ui.add_space(4.0);
        ui.label(format!(
            "Tahmini maliyet: ~{est} token · bugün {used}/{limit}"
        ));
        if used + est > limit {
            ui.label(RichText::new("Günlük limit aşılır; istek reddedilir.").color(DANGER));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Vazgeç").clicked() {
                cancel = true;
            }
            if ui.add(primary_button("Gönder")).clicked() {
                send = true;
            }
        });
    });
    if modal.should_close() {
        cancel = true;
    }
    if send {
        app.ai_confirm = None;
        app.ask_ai(ctx, idx);
    } else if cancel {
        app.ai_confirm = None;
    }
}

pub fn clean(app: &mut App, ui: &mut egui::Ui) {
    heading(ui, "Temizlik", "");

    if let Some(job) = &app.clean_job {
        let done = job.done.load(Relaxed);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Temizleniyor…").size(18.0).strong());
            ui.add(
                egui::ProgressBar::new(done as f32 / job.total.max(1) as f32)
                    .text(format!("{done}/{}", job.total))
                    .fill(ACCENT),
            );
            if ui.button("İptal").clicked() {
                job.cancel.store(true, Relaxed);
            }
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }

    let Some(rep) = &app.report else {
        ui.label("Henüz temizlik yapılmadı. Sonuçlar ekranında öğe seçip “Temizle…” de.");
        return;
    };
    let trashed = rep.count(|o| *o == Outcome::Trashed);
    let deleted = rep.count(|o| *o == Outcome::Deleted);
    let dry = rep.count(|o| *o == Outcome::DryRun);
    let skipped = rep.count(|o| matches!(o, Outcome::Skipped(_)));
    let failed = rep.count(|o| matches!(o, Outcome::Failed(_)));
    ui.horizontal_wrapped(|ui| {
        let label = if rep.dry_run {
            "Silinirdi (kuru çalıştırma)"
        } else {
            "Açılan yer"
        };
        stat(ui, label, &human_bytes(rep.freed_bytes), ACCENT);
        stat(
            ui,
            "Çöpe taşındı / silindi",
            &format!("{trashed} / {deleted}"),
            ACCENT,
        );
        stat(
            ui,
            "Atlandı / hata",
            &format!("{skipped} / {failed}"),
            if failed > 0 { DANGER } else { MUTED },
        );
        if rep.dry_run {
            stat(ui, "Kuru çalıştırma", &format!("{dry} öğe"), WARN);
        }
    });
    ui.add_space(6.0);

    let can_undo = trashed > 0;
    let mut undo = false;
    let mut rescan = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_undo, egui::Button::new("Geri al (Çöp Kutusu'ndan)"))
            .clicked()
        {
            undo = true;
        }
        if ui.add(primary_button("Yeniden tara")).clicked() {
            rescan = true;
        }
        if ui.button("Sonuçlara dön").clicked() {
            app.view = View::Results;
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("İşlem günlüğü").strong());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for e in &rep.entries {
                let (txt, color) = match &e.outcome {
                    Outcome::DryRun => ("silinirdi".to_string(), WARN),
                    Outcome::Trashed => ("çöpe taşındı".to_string(), ACCENT),
                    Outcome::Deleted => ("silindi".to_string(), ACCENT),
                    Outcome::Skipped(m) => (format!("atlandı: {m}"), MUTED),
                    Outcome::Failed(m) => (format!("HATA: {m}"), DANGER),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(txt).color(color));
                    ui.label(RichText::new(human_bytes(e.size)).color(MUTED));
                    ui.add(egui::Label::new(e.path.to_string_lossy()).truncate());
                });
            }
        });

    if undo {
        app.undo_last();
    }
    if rescan {
        let ctx = ui.ctx().clone();
        app.start_scan(&ctx);
    }
}
