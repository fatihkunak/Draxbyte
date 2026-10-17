//! Results table, the confirmation/AI modals and the cleanup screen.

use super::widgets::{card, category_color, danger_button, heading, primary_button, stat};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::{AiNote, App, Confirm, SortKey, View};
use draxbyte::core::cleaner::CleanOptions;
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, now_secs, Category, Outcome};
use draxbyte::tf;
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
        t("Sonuçlar", "Results"),
        t(
            "Kutuları işaretle; hiçbir şey onay ekranından önce silinmez.",
            "Tick the boxes; nothing is deleted before the confirmation screen.",
        ),
    );

    ui.horizontal_wrapped(|ui| {
        for (label, cat) in [
            (t("Tümü", "All"), None),
            (t("Güvenli", "Safe"), Some(Category::Green)),
            (t("Dikkat", "Careful"), Some(Category::Yellow)),
            (t("Silme", "Keep"), Some(Category::Red)),
        ] {
            if ui.selectable_label(app.filter_cat == cat, label).clicked() {
                app.filter_cat = cat;
                app.view_dirty = true;
            }
        }
        if ui
            .add(
                egui::TextEdit::singleline(&mut app.filter_text)
                    .hint_text(t("Yolda ara", "Search path"))
                    .desired_width(180.0),
            )
            .changed()
        {
            app.view_dirty = true;
        }
        ui.separator();
        if ui
            .button(t("Tüm yeşilleri seç", "Select all green"))
            .clicked()
        {
            app.select_category(Category::Green);
        }
        if ui.button(t("Sarıları incele", "Review yellow")).clicked() {
            app.filter_cat = Some(Category::Yellow);
            app.view_dirty = true;
        }
        if ui.button(t("Seçimi temizle", "Clear selection")).clicked() {
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
                    RichText::new(tf!(
                        "{sel_count} öğe seçili · {}",
                        "{sel_count} selected · {}",
                        human_bytes(sel_bytes)
                    ))
                    .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let busy = app.scan.is_some() || app.clean_job.is_some();
                    if ui
                        .add_enabled(
                            sel_count > 0 && !busy,
                            primary_button(t("Temizle…", "Clean…")),
                        )
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
        ui.label(t(
            "Gösterilecek sonuç yok. Ana sayfadan bir tarama başlat.",
            "No results. Start a scan from Home.",
        ));
        return;
    }

    let now = now_secs();
    let mut acts: Vec<Act> = Vec::new();
    let mut new_sort = None;
    let ai_on = app.ai.is_some();
    let ai_tip = app
        .ai_error
        .clone()
        .unwrap_or_else(|| t("AI kapalı (Ayarlar → AI)", "AI is off (Settings → AI)").into());

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
            sortable(&mut h, t("Kategori", "Category"), SortKey::Category, cur);
            sortable(&mut h, t("Boyut", "Size"), SortKey::Size, cur);
            sortable(&mut h, t("Yaş", "Age"), SortKey::Age, cur);
            h.col(|ui| {
                ui.label(RichText::new(t("Neden", "Reason")).strong());
            });
            sortable(&mut h, t("Yol", "Path"), SortKey::Path, cur);
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
                        tf!("{days} g", "{days} d")
                    });
                });
                row.col(|ui| {
                    ui.label(
                        RichText::new(&draxbyte::i18n::rule_reason(&r.item.rule, &r.item.reason))
                            .color(MUTED)
                            .small(),
                    )
                    .on_hover_text(&draxbyte::i18n::rule_reason(&r.item.rule, &r.item.reason));
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
                        let tag = if *cached {
                            t(" (önbellek)", " (cached)")
                        } else {
                            ""
                        };
                        ui.add(
                            egui::Label::new(RichText::new(format!("{text}{tag}")).small())
                                .truncate(),
                        )
                        .on_hover_text(text);
                    }
                    None => {
                        let hint = if r.item.unknown {
                            t("Belirsiz öğe — AI'ya sor", "Unclear item — ask AI")
                        } else {
                            t("AI'ya sor", "Ask AI")
                        };
                        let btn =
                            egui::Button::new(RichText::new(t("AI'ya Sor", "Ask AI")).small());
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
        ui.label(
            RichText::new(t("Temizliği onayla", "Confirm cleanup"))
                .size(20.0)
                .strong(),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(tf!(
                "{n} öğe, {} silinecek",
                "{n} items, {} will be deleted",
                human_bytes(bytes)
            ))
            .size(16.0),
        );
        ui.label(RichText::new(tf!("Yeşil: {}", "Green: {}", human_bytes(green_b))).color(ACCENT));
        if yellow_n > 0 {
            ui.label(
                RichText::new(tf!(
                    "Dikkat (sarı): {yellow_n} öğe, {} — emin ol",
                    "Careful (yellow): {yellow_n} items, {} — make sure",
                    human_bytes(yellow_b)
                ))
                .color(WARN),
            );
        }
        ui.add_space(8.0);
        ui.checkbox(
            &mut c.dry_run,
            t(
                "Kuru çalıştırma (hiçbir şey silme, sadece raporla)",
                "Dry run (delete nothing, just report)",
            ),
        );
        if !c.dry_run {
            ui.radio_value(
                &mut c.permanent,
                false,
                t(
                    "Çöp Kutusu'na taşı (geri alınabilir)",
                    "Move to Recycle Bin (reversible)",
                ),
            );
            ui.radio_value(
                &mut c.permanent,
                true,
                RichText::new(t(
                    "Kalıcı sil (geri alınamaz)",
                    "Delete permanently (irreversible)",
                ))
                .color(DANGER),
            );
            if c.permanent && need_typed {
                ui.label(
                    RichText::new(t("Onaylamak için SIL yaz:", "Type DELETE to confirm:"))
                        .color(DANGER),
                );
                ui.text_edit_singleline(&mut c.typed);
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(t("Vazgeç", "Cancel")).clicked() {
                cancel = true;
            }
            let permanent = !c.dry_run && c.permanent;
            let ok = !(permanent && need_typed && c.typed.trim() != draxbyte::i18n::confirm_word());
            let btn = if permanent {
                danger_button(t("Kalıcı sil", "Delete permanently"))
            } else {
                primary_button(if c.dry_run {
                    t("Kuru çalıştır", "Run dry run")
                } else {
                    t("Çöpe taşı", "Move to Bin")
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
        ui.label(
            RichText::new(t("AI'ya sor — onay", "Ask AI — approval"))
                .size(20.0)
                .strong(),
        );
        ui.label(
            RichText::new(t(
                "Yalnızca aşağıdaki metin gönderilir (dosya içeriği değil):",
                "Only the text below is sent (not file contents):",
            ))
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
        ui.label(tf!(
            "Tahmini maliyet: ~{est} token · bugün {used}/{limit}",
            "Estimated cost: ~{est} tokens · today {used}/{limit}"
        ));
        if used + est > limit {
            ui.label(
                RichText::new(t(
                    "Günlük limit aşılır; istek reddedilir.",
                    "The daily limit would be exceeded; the request will be refused.",
                ))
                .color(DANGER),
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(t("Vazgeç", "Cancel")).clicked() {
                cancel = true;
            }
            if ui.add(primary_button(t("Gönder", "Send"))).clicked() {
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
    heading(ui, t("Temizlik", "Cleanup"), "");

    if let Some(job) = &app.clean_job {
        let done = job.done.load(Relaxed);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(t("Temizleniyor…", "Cleaning…"))
                    .size(18.0)
                    .strong(),
            );
            ui.add(
                egui::ProgressBar::new(done as f32 / job.total.max(1) as f32)
                    .text(format!("{done}/{}", job.total))
                    .fill(ACCENT),
            );
            if ui.button(t("İptal", "Cancel")).clicked() {
                job.cancel.store(true, Relaxed);
            }
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }

    let Some(rep) = &app.report else {
        ui.label(t(
            "Henüz temizlik yapılmadı. Sonuçlar ekranında öğe seçip “Temizle…” de.",
            "No cleanup yet. Select items on the Results screen and press “Clean…”.",
        ));
        return;
    };
    let trashed = rep.count(|o| *o == Outcome::Trashed);
    let deleted = rep.count(|o| *o == Outcome::Deleted);
    let dry = rep.count(|o| *o == Outcome::DryRun);
    let skipped = rep.count(|o| matches!(o, Outcome::Skipped(_)));
    let failed = rep.count(|o| matches!(o, Outcome::Failed(_)));
    ui.horizontal_wrapped(|ui| {
        let label = if rep.dry_run {
            t("Silinirdi (kuru çalıştırma)", "Would delete (dry run)")
        } else {
            t("Açılan yer", "Space freed")
        };
        stat(ui, label, &human_bytes(rep.freed_bytes), ACCENT);
        stat(
            ui,
            t("Çöpe taşındı / silindi", "Moved to Bin / deleted"),
            &format!("{trashed} / {deleted}"),
            ACCENT,
        );
        stat(
            ui,
            t("Atlandı / hata", "Skipped / errors"),
            &format!("{skipped} / {failed}"),
            if failed > 0 { DANGER } else { MUTED },
        );
        if rep.dry_run {
            stat(
                ui,
                t("Kuru çalıştırma", "Dry run"),
                &tf!("{dry} öğe", "{dry} items"),
                WARN,
            );
        }
    });
    ui.add_space(6.0);

    let can_undo = trashed > 0;
    let mut undo = false;
    let mut rescan = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                can_undo,
                egui::Button::new(t("Geri al (Çöp Kutusu'ndan)", "Undo (from Recycle Bin)")),
            )
            .clicked()
        {
            undo = true;
        }
        if ui
            .add(primary_button(t("Yeniden tara", "Rescan")))
            .clicked()
        {
            rescan = true;
        }
        if ui.button(t("Sonuçlara dön", "Back to results")).clicked() {
            app.view = View::Results;
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t("İşlem günlüğü", "Operation log")).strong());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for e in &rep.entries {
                let (txt, color) = match &e.outcome {
                    Outcome::DryRun => (t("silinirdi", "would delete").to_string(), WARN),
                    Outcome::Trashed => (t("çöpe taşındı", "moved to Bin").to_string(), ACCENT),
                    Outcome::Deleted => (t("silindi", "deleted").to_string(), ACCENT),
                    Outcome::Skipped(m) => (tf!("atlandı: {m}", "skipped: {m}"), MUTED),
                    Outcome::Failed(m) => (tf!("HATA: {m}", "ERROR: {m}"), DANGER),
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
