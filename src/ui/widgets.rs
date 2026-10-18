//! Small reusable pieces: logo, card, ring chart, category pill.

use super::{ACCENT, BORDER, CARD, DANGER, MUTED, TEXT, WARN};
use draxbyte::models::Category;
use egui::{epaint::PathStroke, Color32, Pos2, RichText, Shape, Stroke, Ui, Vec2};

pub fn category_color(c: Category) -> Color32 {
    match c {
        Category::Green => ACCENT,
        Category::Yellow => WARN,
        Category::Red => DANGER,
    }
}

pub fn logo(ui: &mut Ui, tex: &egui::TextureHandle) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::splat(32.0)));
        ui.add_space(6.0);
        ui.label(RichText::new("Drax").font(super::bold(20.0)));
        ui.label(RichText::new("byte").font(super::bold(20.0)).color(ACCENT));
    });
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(16))
        .show(ui, add)
}

pub fn heading(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).font(super::bold(24.0)));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(MUTED));
    }
    ui.add_space(10.0);
}

/// Donut chart: `fraction` of the circle is drawn in `color` over a dark track.
pub fn ring(ui: &mut Ui, size: f32, fraction: f32, color: Color32, big: &str, small: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let center = rect.center();
    let width = size * 0.085;
    let radius = size * 0.5 - width;
    let painter = ui.painter();
    painter.circle_stroke(center, radius, Stroke::new(width, BORDER));
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let steps = ((f * 120.0) as usize).max(2);
        let start = -std::f32::consts::FRAC_PI_2;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = start + f * std::f32::consts::TAU * (i as f32 / steps as f32);
                center + Vec2::angled(a) * radius
            })
            .collect();
        painter.add(Shape::line(pts, PathStroke::new(width, color)));
    }
    painter.text(
        center - Vec2::new(0.0, size * 0.04),
        egui::Align2::CENTER_CENTER,
        big,
        egui::FontId::proportional(size * 0.15),
        TEXT,
    );
    painter.text(
        center + Vec2::new(0.0, size * 0.1),
        egui::Align2::CENTER_CENTER,
        small,
        egui::FontId::proportional(size * 0.07),
        MUTED,
    );
}

pub fn stat(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    card(ui, |ui| {
        ui.set_min_width(150.0);
        ui.label(RichText::new(label).color(MUTED));
        ui.label(RichText::new(value).size(22.0).strong().color(color));
    });
}

pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_string())
            .color(Color32::from_rgb(0x06, 0x2B, 0x20))
            .strong(),
    )
    .fill(ACCENT)
}

pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_string())
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(Color32::from_rgb(0xB9, 0x1C, 0x1C))
}

/// Multi-segment donut. `segments` are (fraction of the whole, colour); the rest is the dark track.
pub fn donut(ui: &mut Ui, size: f32, segments: &[(f32, Color32)], big: &str, small: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let center = rect.center();
    let width = size * 0.12;
    let radius = size * 0.5 - width * 0.6;
    let painter = ui.painter();
    painter.circle_stroke(center, radius, Stroke::new(width, BORDER));
    let gap = 0.012; // fraction of the circle left between segments
    let mut start = 0.0f32;
    for (frac, color) in segments {
        let f = (*frac - gap).max(0.0);
        if f > 0.001 {
            let steps = ((f * 160.0) as usize).max(2);
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + (start + gap / 2.0 + f * i as f32 / steps as f32) * std::f32::consts::TAU;
                    center + Vec2::angled(a) * radius
                })
                .collect();
            painter.add(Shape::line(pts, PathStroke::new(width, *color)));
        }
        start += *frac;
    }
    painter.text(
        center - Vec2::new(0.0, size * 0.05),
        egui::Align2::CENTER_CENTER,
        big,
        super::bold(size * 0.15),
        TEXT,
    );
    painter.text(
        center + Vec2::new(0.0, size * 0.1),
        egui::Align2::CENTER_CENTER,
        small,
        egui::FontId::proportional(size * 0.075),
        MUTED,
    );
}

#[derive(Clone, Copy)]
pub enum Icon {
    Donut,
    Folder,
    Trash,
    Bolt,
}

fn paint_icon(p: &egui::Painter, r: egui::Rect, icon: Icon) {
    let c = r.center();
    let s = Stroke::new(1.8, ACCENT);
    match icon {
        Icon::Donut => {
            p.circle_stroke(c, 8.0, s);
            p.circle_filled(c, 2.5, ACCENT);
        }
        Icon::Folder => {
            let b = egui::Rect::from_center_size(c + Vec2::new(0.0, 1.5), Vec2::new(18.0, 13.0));
            p.rect_stroke(b, 3.0, s, egui::StrokeKind::Middle);
            p.line_segment(
                [
                    b.left_top() + Vec2::new(1.0, -3.0),
                    b.left_top() + Vec2::new(7.0, -3.0),
                ],
                s,
            );
        }
        Icon::Trash => {
            let b = egui::Rect::from_center_size(c + Vec2::new(0.0, 2.5), Vec2::new(13.0, 14.0));
            p.rect_stroke(b, 2.5, s, egui::StrokeKind::Middle);
            p.line_segment([c + Vec2::new(-9.0, -6.0), c + Vec2::new(9.0, -6.0)], s);
            p.line_segment([c + Vec2::new(-2.5, -6.0), c + Vec2::new(-2.5, -9.0)], s);
            p.line_segment([c + Vec2::new(2.5, -6.0), c + Vec2::new(2.5, -9.0)], s);
        }
        Icon::Bolt => {
            let pts = [
                c + Vec2::new(2.0, -10.0),
                c + Vec2::new(-6.0, 2.0),
                c + Vec2::new(0.0, 2.0),
                c + Vec2::new(-2.0, 10.0),
                c + Vec2::new(7.0, -3.0),
                c + Vec2::new(1.0, -3.0),
                c + Vec2::new(2.0, -10.0),
            ];
            p.add(Shape::line(pts.to_vec(), s));
        }
    }
}

/// Clickable dashboard tile. Returns true when clicked.
pub fn action_card(ui: &mut Ui, icon: Icon, title: &str, desc: &str) -> bool {
    let width = ui.available_width();
    let height = 112.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter();
    p.rect(
        rect,
        12.0,
        CARD,
        Stroke::new(1.0, if hovered { ACCENT } else { BORDER }),
        egui::StrokeKind::Inside,
    );
    let ib = egui::Rect::from_min_size(rect.min + Vec2::new(16.0, 16.0), Vec2::splat(34.0));
    p.rect_filled(ib, 9.0, super::ACCENT_DIM);
    paint_icon(p, ib, icon);
    p.text(
        rect.min + Vec2::new(16.0, 62.0),
        egui::Align2::LEFT_CENTER,
        title,
        super::bold(15.5),
        TEXT,
    );
    // keep the description inside the card even when the window is narrow
    let clipped = p.with_clip_rect(rect.shrink2(Vec2::new(12.0, 0.0)));
    clipped.text(
        rect.min + Vec2::new(16.0, 86.0),
        egui::Align2::LEFT_CENTER,
        desc,
        egui::FontId::proportional(12.5),
        MUTED,
    );
    p.text(
        rect.right_top() + Vec2::new(-20.0, 33.0),
        egui::Align2::RIGHT_CENTER,
        "→",
        egui::FontId::proportional(16.0),
        if hovered { ACCENT } else { MUTED },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

pub fn kind_color(k: draxbyte::models::Kind) -> Color32 {
    use draxbyte::models::Kind::*;
    match k {
        Video => Color32::from_rgb(0x3B, 0x82, 0xF6),
        Image => Color32::from_rgb(0xF5, 0xB3, 0x01),
        Audio => Color32::from_rgb(0xEC, 0x48, 0x99),
        Document => Color32::from_rgb(0xA7, 0x8B, 0xFA),
        Archive => Color32::from_rgb(0xF9, 0x73, 0x16),
        Developer => Color32::from_rgb(0x10, 0xB9, 0x81),
        Cache => Color32::from_rgb(0x22, 0xD3, 0xEE),
        VmDisk => Color32::from_rgb(0x6D, 0x7B, 0xFF),
        Other => Color32::from_rgb(0x9C, 0xA3, 0xAF),
    }
}

/// Small coloured square with the kind's initial (file-type badge).
pub fn kind_badge(ui: &mut Ui, k: draxbyte::models::Kind) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::hover());
    let c = kind_color(k);
    ui.painter().rect_filled(rect, 8.0, c.gamma_multiply(0.22));
    let initial = k.label().chars().next().unwrap_or('?').to_string();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        super::bold(14.0),
        c,
    );
}

/// Like `stat` but fills the width of its column.
pub fn stat_fill(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(label).color(MUTED));
        ui.label(RichText::new(value).size(22.0).strong().color(color));
    });
}

/// Shows a file or folder in the system file manager (Explorer / Finder / the folder on Linux).
pub fn reveal(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

/// Opens the system page where Full Disk Access is granted (macOS); elsewhere does nothing.
pub fn open_full_disk_access() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn();
    }
}
