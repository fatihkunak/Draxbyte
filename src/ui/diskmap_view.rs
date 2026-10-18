//! Disk map screen: a treemap of what takes the space. Click a folder to look inside it.

use super::widgets::{card, heading, kind_color, primary_button, reveal};
use super::{ACCENT, MUTED, WARN};
use crate::app::App;
use draxbyte::core::diskmap::{squarify, Node, Rect};
use draxbyte::i18n::t;
use draxbyte::models::{human_bytes, Category, ScanItem};
use draxbyte::tf;
use egui::{Color32, RichText};
use std::sync::atomic::Ordering::Relaxed;

const DIR_COLORS: [Color32; 6] = [
    Color32::from_rgb(0x0E, 0x7C, 0x5E),
    Color32::from_rgb(0x1F, 0x5F, 0xB5),
    Color32::from_rgb(0x6B, 0x4E, 0xB0),
    Color32::from_rgb(0xB0, 0x7A, 0x0A),
    Color32::from_rgb(0xA3, 0x3E, 0x5C),
    Color32::from_rgb(0x2C, 0x8F, 0x9C),
];

fn tile_color(n: &Node, idx: usize) -> Color32 {
    if n.other {
        return super::BORDER;
    }
    if n.is_dir {
        return DIR_COLORS[idx % DIR_COLORS.len()];
    }
    // file: colour by type
    let item = ScanItem {
        path: n.path.clone(),
        size: n.size,
        mtime: 0,
        is_dir: false,
        category: Category::Yellow,
        rule: String::new(),
        reason: String::new(),
        unknown: false,
    };
    kind_color(item.kind()).gamma_multiply(0.85)
}

pub fn disk_map(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("Disk Haritası", "Disk Map"),
        t(
            "Alan neyin altında? Büyük dikdörtgen = büyük klasör ya da dosya. Bir klasöre tıklayınca içine girersin.",
            "What is taking the space? A bigger tile is a bigger folder or file. Click a folder to look inside it.",
        ),
    );
    let ctx = ui.ctx().clone();

    // ---- controls
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t("Harita:", "Map:")).color(MUTED));
            ui.add(egui::TextEdit::singleline(&mut app.map.root_text).desired_width(300.0));
            if ui.small_button(t("Ev klasörü", "Home")).clicked() {
                app.map.root_text = "~".into();
            }
            for d in draxbyte::core::syslocs::fixed_drive_roots() {
                let label = d.to_string_lossy().into_owned();
                if ui.small_button(&label).clicked() {
                    app.map.root_text = label;
                }
            }
            let busy = app.map.job.is_some();
            if busy {
                if ui.button(t("İptal", "Cancel")).clicked() {
                    if let Some(j) = &app.map.job {
                        j.cancel.store(true, Relaxed);
                    }
                }
            } else if ui.add(primary_button(t("Haritayı oluştur", "Build map"))).clicked() {
                app.map_start(&ctx);
            }
        });
        if let Some(j) = &app.map.job {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(tf!(
                        "{} dosya · {} · {} sn",
                        "{} files · {} · {} s",
                        j.progress.files.load(Relaxed),
                        human_bytes(j.progress.bytes.load(Relaxed)),
                        j.started.elapsed().as_secs()
                    ))
                    .color(MUTED),
                );
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if let Some(m) = &app.map.msg {
            ui.label(RichText::new(m).color(WARN));
        }
    });
    ui.add_space(8.0);

    if app.map.tree.is_none() {
        if app.map.job.is_none() {
            ui.label(RichText::new(t("Bir klasör ya da sürücü seç ve \"Haritayı oluştur\"a bas. Büyük sürücüler birkaç dakika sürebilir; hiçbir şey değişmez.", "Pick a folder or drive and press \"Build map\". Big drives can take a few minutes; nothing is changed.")).color(MUTED));
        }
        return;
    }

    if app.map.errors > 0 {
        let elevated = draxbyte::core::elevate::is_elevated();
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tf!("{} klasör okunamadı; boyutları haritada eksik olabilir.", "{} folders could not be read; their sizes may be missing from the map.", app.map.errors)).color(WARN));
            super::scan_view::access_buttons(ui, elevated);
        });
        ui.add_space(6.0);
    }

    // ---- breadcrumb
    let mut jump: Option<usize> = None;
    {
        let tree = app.map.tree.as_ref().unwrap();
        let mut names = vec![tree.name.clone()];
        let mut n = tree;
        for &i in &app.map.path {
            n = &n.children[i];
            names.push(n.name.clone());
        }
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!app.map.path.is_empty(), egui::Button::new(t("↑ Yukarı", "↑ Up"))).clicked() {
                jump = Some(app.map.path.len().saturating_sub(1));
            }
            for (i, name) in names.iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").color(MUTED));
                }
                if i + 1 == names.len() {
                    ui.label(RichText::new(name).strong());
                } else if ui.link(name).clicked() {
                    jump = Some(i);
                }
            }
        });
    }
    if let Some(j) = jump {
        app.map.path.truncate(j);
    }
    let Some(cur) = app.map.current().cloned() else { return };
    ui.label(
        RichText::new(tf!(
            "{} · {} dosya · yalnızca büyük öğeler ayrı gösterilir, kalanı \"küçük öğe\" kutusunda toplanır",
            "{} · {} files · only the big items are shown separately; the rest is folded into the \"small items\" tile",
            human_bytes(cur.size),
            cur.files
        ))
        .color(MUTED)
        .small(),
    );
    ui.add_space(4.0);

    // ---- treemap
    let avail = ui.available_size();
    let h = (avail.y - 210.0).clamp(240.0, 520.0);
    let (area, resp) = ui.allocate_exact_size(egui::vec2(avail.x, h), egui::Sense::click());
    let sizes: Vec<u64> = cur.children.iter().map(|c| c.size).collect();
    let rects = squarify(&sizes, Rect { x: area.min.x, y: area.min.y, w: area.width(), h: area.height() });
    let hover_pos = resp.hover_pos();
    let mut hovered: Option<usize> = None;
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 4.0, super::SIDEBAR);
    for (i, (c, r)) in cur.children.iter().zip(&rects).enumerate() {
        if r.area() < 1.0 {
            continue;
        }
        let er = egui::Rect::from_min_size(egui::pos2(r.x, r.y), egui::vec2(r.w, r.h)).shrink(1.5);
        if er.width() < 1.0 || er.height() < 1.0 {
            continue;
        }
        let is_hover = hover_pos.map(|p| er.contains(p)).unwrap_or(false);
        if is_hover {
            hovered = Some(i);
        }
        let col = tile_color(c, i);
        painter.rect_filled(er, 4.0, if is_hover { col.gamma_multiply(1.25) } else { col });
        if is_hover {
            painter.rect_stroke(er, 4.0, egui::Stroke::new(2.0, super::TEXT), egui::StrokeKind::Inside);
        }
        if er.width() > 74.0 && er.height() > 34.0 {
            let clip = painter.with_clip_rect(er.shrink(4.0));
            clip.text(er.min + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, &c.name, super::bold(13.0), Color32::WHITE);
            clip.text(er.min + egui::vec2(8.0, 22.0), egui::Align2::LEFT_TOP, human_bytes(c.size), egui::FontId::proportional(12.0), Color32::from_white_alpha(210));
        }
    }
    if resp.clicked() {
        if let Some(i) = hovered {
            app.map.enter(i);
        }
    }
    if let Some(i) = hovered {
        let c = &cur.children[i];
        let pct = if cur.size > 0 { c.size as f64 * 100.0 / cur.size as f64 } else { 0.0 };
        ui.label(
            RichText::new(format!(
                "{} · {} · %{:.1}{}",
                c.path.display(),
                human_bytes(c.size),
                pct,
                if c.is_dir && !c.other { tf!(" · {} dosya", " · {} files", c.files) } else { String::new() }
            ))
            .color(ACCENT),
        );
    } else {
        ui.label(RichText::new(t("Üzerine gel: ayrıntı · tıkla: içine gir", "Hover for details · click to open a folder")).color(MUTED));
    }
    ui.add_space(6.0);

    // ---- biggest entries as a list
    let mut open: Option<usize> = None;
    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
        for (i, c) in cur.children.iter().enumerate().take(14) {
            ui.horizontal(|ui| {
                let (chip, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(chip, 3.0, tile_color(c, i));
                let pct = if cur.size > 0 { c.size as f32 / cur.size as f32 } else { 0.0 };
                ui.allocate_ui_with_layout(egui::vec2(210.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(c.name.as_str()).truncate());
                });
                ui.add_sized([80.0, 18.0], egui::Label::new(human_bytes(c.size)));
                ui.add_sized([56.0, 18.0], egui::Label::new(RichText::new(format!("%{:.1}", pct * 100.0)).color(MUTED)));
                if c.is_dir && !c.other && !c.children.is_empty() && ui.small_button(t("Aç", "Open")).clicked() {
                    open = Some(i);
                }
                if !c.other && ui.small_button(t("Göster", "Show")).on_hover_text(t("Dosya yöneticisinde göster", "Show in the file manager")).clicked() {
                    reveal(&c.path);
                }
            });
        }
    });
    if let Some(i) = open {
        app.map.enter(i);
    }
}
