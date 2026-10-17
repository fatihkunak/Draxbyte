//! Home screen and the live scan-progress screen.

use super::widgets::{
    action_card, card, category_color, donut, heading, kind_badge, kind_color, primary_button,
    ring, stat_fill, Icon,
};
use super::{ACCENT, BLUE, MUTED, WARN};
use crate::app::{App, Confirm, SortKey, View};
use draxbyte::models::{human_bytes, now_secs, Category, Kind};
use egui::RichText;
use egui_extras::{Size, StripBuilder};
use std::sync::atomic::Ordering::Relaxed;

fn ago(ts: i64) -> String {
    let d = (now_secs() - ts).max(0);
    match d {
        0..=59 => "az önce".into(),
        60..=3599 => format!("{} dk önce", d / 60),
        3600..=86_399 => format!("{} saat önce", d / 3600),
        _ => format!("{} gün önce", d / 86_400),
    }
}

/// Sizes of the cleanable (non-red) findings grouped by kind, largest first.
fn distribution(app: &App) -> Vec<(Kind, u64)> {
    let mut by: std::collections::HashMap<Kind, u64> = std::collections::HashMap::new();
    for r in app.rows.iter().filter(|r| r.item.category != Category::Red) {
        *by.entry(r.item.kind()).or_default() += r.item.size;
    }
    let mut v: Vec<_> = by.into_iter().filter(|(_, b)| *b > 0).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

pub fn home(app: &mut App, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            hero(app, ui);
            ui.add_space(14.0);
            quick_actions(app, ui);
            ui.add_space(14.0);
            ui.columns(2, |cols| {
                top_files(app, &mut cols[0]);
                distribution_card(app, &mut cols[1]);
            });
            ui.add_space(14.0);
            last_scan(app, ui);
        });
}

fn hero(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        // StripBuilder fills all available height; pin it to the hero's natural height.
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 205.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        StripBuilder::new(ui).size(Size::remainder()).size(Size::exact(430.0)).horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Depolama alanınızı geri kazanın.").font(super::bold(30.0)));
                ui.add_space(4.0);
                ui.label(RichText::new("Draxbyte diski yerelde tarar, büyük ve gereksiz dosyaları bulur; hiçbir şeyi onayın olmadan silmez. Yapay zekâ kapalıyken hiçbir ağ isteği atılmaz.").color(MUTED));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let busy = app.scan.is_some() || app.clean_job.is_some();
                    if ui.add_enabled(!busy, primary_button("Hızlı tarama")).clicked() {
                        let ctx = ui.ctx().clone();
                        app.start_scan(&ctx);
                    }
                    if ui.button("Detaylı analiz").clicked() {
                        app.view = View::Results;
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new("Taranacak yollar (; ile ayır)").color(MUTED).small());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.scan_paths_text).desired_width((ui.available_width() - 130.0).max(120.0)));
                    if ui.button("Varsayılan").clicked() {
                        app.scan_paths_text = app.cfg.scan.default_paths.join("; ");
                    }
                });
            });
            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    if let Some(d) = &app.disk {
                        let used = d.total.saturating_sub(d.free);
                        let frac = if d.total > 0 { used as f32 / d.total as f32 } else { 0.0 };
                        ring(ui, 190.0, frac, ACCENT, &human_bytes(d.total), &format!("{} · %{:.0} dolu", d.name, frac * 100.0));
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            ui.add_space(52.0);
                            ui.label(RichText::new("• Kullanılan").color(ACCENT));
                            ui.label(RichText::new(human_bytes(used)).font(super::bold(17.0)));
                            ui.add_space(6.0);
                            ui.label(RichText::new("• Boş alan").color(BLUE));
                            ui.label(RichText::new(human_bytes(d.free)).font(super::bold(17.0)));
                        });
                    }
                });
            });
        });
        });
    });
}

fn quick_actions(app: &mut App, ui: &mut egui::Ui) {
    let mut act: Option<u8> = None;
    ui.columns(4, |c| {
        if action_card(
            &mut c[0],
            Icon::Donut,
            "Disk analizi",
            "Bulunan her şeyi boyuta göre gör.",
        ) {
            act = Some(0);
        }
        if action_card(
            &mut c[1],
            Icon::Folder,
            "Büyük dosyalar",
            "Ne olduğu belirsiz büyük öğeler.",
        ) {
            act = Some(1);
        }
        if action_card(
            &mut c[2],
            Icon::Trash,
            "Geçici dosyalar",
            "Güvenle silinebilecekler.",
        ) {
            act = Some(2);
        }
        if action_card(
            &mut c[3],
            Icon::Bolt,
            "Sistem temizliği",
            "Yeşilleri seç ve temizle.",
        ) {
            act = Some(3);
        }
    });
    let Some(a) = act else { return };
    if app.rows.is_empty() {
        let ctx = ui.ctx().clone();
        app.start_scan(&ctx);
        return;
    }
    app.filter_text.clear();
    app.view_dirty = true;
    app.view = View::Results;
    app.sort = (SortKey::Size, false);
    match a {
        0 => app.filter_cat = None,
        1 => app.filter_cat = Some(Category::Yellow),
        2 => app.filter_cat = Some(Category::Green),
        _ => {
            app.filter_cat = Some(Category::Green);
            app.select_category(Category::Green);
            if app.selected().next().is_some() {
                app.confirm = Some(Confirm {
                    dry_run: false,
                    permanent: !app.cfg.cleanup.use_trash,
                    typed: String::new(),
                });
            }
        }
    }
}

fn top_files(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(262.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("En büyük temizlenebilir öğeler").font(super::bold(16.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .link(RichText::new("Tümünü gör →").color(ACCENT))
                    .clicked()
                {
                    app.filter_cat = None;
                    app.view_dirty = true;
                    app.view = View::Results;
                }
            });
        });
        ui.add_space(6.0);
        let top: Vec<&crate::app::Row> = app
            .rows
            .iter()
            .filter(|r| r.item.category != Category::Red)
            .take(5)
            .collect();
        if top.is_empty() {
            ui.add_space(30.0);
            ui.label(RichText::new("Henüz veri yok. “Hızlı tarama” ile başla.").color(MUTED));
        }
        for r in top {
            let fname = r
                .item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| r.item.path.display().to_string());
            // "build", "Cache", "target" say nothing on their own: prefix the parent folder.
            let generic = matches!(
                fname.to_lowercase().as_str(),
                "build" | "cache" | "caches" | "target" | "node_modules" | "temp" | "tmp" | ".cxx"
            );
            let parent_name = r
                .item
                .path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            let name = match (generic, parent_name) {
                (true, Some(pn)) => format!("{pn}{}{fname}", std::path::MAIN_SEPARATOR),
                _ => fname,
            };
            let parent = r
                .item
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                kind_badge(ui, r.item.kind());
                let text_w = (ui.available_width() - 110.0).max(60.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(text_w, 34.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                        ui.add(
                            egui::Label::new(RichText::new(parent).color(MUTED).small()).truncate(),
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(human_bytes(r.item.size))
                            .color(category_color(r.item.category)),
                    );
                });
            });
            ui.add_space(2.0);
        }
    });
}

fn distribution_card(app: &mut App, ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(262.0);
        ui.label(RichText::new("Temizlenebilir öğelerin dağılımı").font(super::bold(16.0)));
        ui.label(
            RichText::new("Son taramada bulunanlar (kırmızılar hariç)")
                .color(MUTED)
                .small(),
        );
        ui.add_space(8.0);
        let dist = distribution(app);
        let total: u64 = dist.iter().map(|(_, b)| *b).sum();
        if total == 0 {
            ui.add_space(30.0);
            ui.label(RichText::new("Henüz veri yok.").color(MUTED));
            return;
        }
        let narrow = ui.available_width() < 520.0;
        ui.horizontal(|ui| {
            let segs: Vec<(f32, egui::Color32)> = dist
                .iter()
                .map(|(k, b)| (*b as f32 / total as f32, kind_color(*k)))
                .collect();
            donut(
                ui,
                if narrow { 130.0 } else { 180.0 },
                &segs,
                &human_bytes(total),
                "toplam",
            );
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let max = dist.first().map(|(_, b)| *b).unwrap_or(1).max(1) as f32;
                for (k, b) in dist.iter().take(7) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("•").color(kind_color(*k)));
                        ui.add_sized(
                            [if narrow { 104.0 } else { 128.0 }, 18.0],
                            egui::Label::new(k.label()).truncate(),
                        );
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2((ui.available_width() - 120.0).clamp(24.0, 220.0), 8.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 4.0, super::BORDER);
                        let mut fill = rect;
                        fill.set_width((rect.width() * (*b as f32 / max)).max(3.0));
                        ui.painter().rect_filled(fill, 4.0, kind_color(*k));
                        ui.label(
                            RichText::new(format!(
                                "{}  %{:.0}",
                                human_bytes(*b),
                                *b as f32 * 100.0 / total as f32
                            ))
                            .small()
                            .color(MUTED),
                        );
                    });
                }
            });
        });
    });
}

fn last_scan(app: &mut App, ui: &mut egui::Ui) {
    let sum = |c: Category| {
        app.rows
            .iter()
            .filter(|r| r.item.category == c)
            .map(|r| r.item.size)
            .sum::<u64>()
    };
    let count = |c: Category| app.rows.iter().filter(|r| r.item.category == c).count();
    ui.columns(3, |cols| {
        stat_fill(
            &mut cols[0],
            "Güvenli (yeşil)",
            &format!(
                "{} · {} öğe",
                human_bytes(sum(Category::Green)),
                count(Category::Green)
            ),
            ACCENT,
        );
        stat_fill(
            &mut cols[1],
            "Dikkat (sarı)",
            &format!(
                "{} · {} öğe",
                human_bytes(sum(Category::Yellow)),
                count(Category::Yellow)
            ),
            WARN,
        );
        stat_fill(
            &mut cols[2],
            "Silme (kırmızı)",
            &format!(
                "{} · {} öğe",
                human_bytes(sum(Category::Red)),
                count(Category::Red)
            ),
            super::DANGER,
        );
    });
    ui.add_space(14.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        match &app.last {
            Some(s) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Son tarama").font(super::bold(16.0)));
                    ui.label(RichText::new(ago(s.finished_at)).color(MUTED));
                });
                ui.label(format!(
                    "{} dosya gezildi · {} öğe ({}) · {} erişilemeyen",
                    s.files_visited,
                    s.item_count,
                    human_bytes(s.total_bytes),
                    s.errors
                ));
                ui.label(RichText::new(s.roots.join("   |   ")).color(MUTED).small());
                if ui.button("Sonuçları gör").clicked() {
                    app.view = View::Results;
                }
            }
            None => {
                ui.label("Henüz tarama yok. “Hızlı tarama” ile başla.");
            }
        }
    });
}

pub fn progress(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        "Tarama",
        "Dosyalar yerelde taranıyor; hiçbir şey silinmez.",
    );
    let Some(job) = &app.scan else {
        ui.label("Çalışan tarama yok.");
        if ui.add(primary_button("Ana sayfaya dön")).clicked() {
            app.view = View::Home;
        }
        return;
    };
    let files = job.progress.files.load(Relaxed);
    let bytes = job.progress.bytes.load(Relaxed);
    let found = job.progress.found.load(Relaxed);
    let errors = job.progress.errors.load(Relaxed);
    let current = job
        .progress
        .current
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let secs = job.started.elapsed().as_secs_f32();
    let cancelling = job.cancel.load(Relaxed);

    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(28.0).color(ACCENT));
            ui.label(
                RichText::new(if cancelling {
                    "İptal ediliyor…"
                } else {
                    "Taranıyor…"
                })
                .size(18.0)
                .strong(),
            );
        });
        ui.add_space(8.0);
        ui.label(format!(
            "{files} dosya · {} gezildi · {found} aday öğe · {errors} erişilemeyen · {secs:.0} sn",
            human_bytes(bytes)
        ));
        ui.label(RichText::new(current).color(MUTED).small());
        ui.add_space(8.0);
        if ui
            .add_enabled(!cancelling, egui::Button::new("İptal"))
            .clicked()
        {
            app.cancel_scan();
        }
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
