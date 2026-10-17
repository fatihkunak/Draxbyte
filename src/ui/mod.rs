//! Theme, shell (sidebar + content) and view dispatch. Palette follows the brand sheet.

pub mod about_view;
pub mod result_view;
pub mod scan_view;
pub mod settings_view;
pub mod widgets;

use crate::app::{App, View};
use draxbyte::i18n::t;
use egui::{Color32, RichText};

/// 256x256 RGBA of assets/icon.ico (rebuilt by scripts/make_icon.py).
pub const ICON_RGBA: &[u8] = include_bytes!("../../assets/icon_256.rgba");

pub const ACCENT: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x12, 0x3A, 0x30);
pub const BG: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x21);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x14, 0x16, 0x1A);
pub const CARD: Color32 = Color32::from_rgb(0x22, 0x26, 0x2B);
pub const BORDER: Color32 = Color32::from_rgb(0x2B, 0x2F, 0x36);
pub const TEXT: Color32 = Color32::from_rgb(0xF3, 0xF4, 0xF6);
pub const MUTED: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);
pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xB3, 0x01);
pub const DANGER: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);
pub const BLUE: Color32 = Color32::from_rgb(0x3B, 0x82, 0xF6);

static INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");

/// Inter SemiBold at `size` (headings, brand text).
pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("inter-semibold".into()))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_SEMIBOLD)),
    );
    // Inter first; egui's bundled fonts stay behind it as fallback for any missing glyph.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    let mut bold_stack = vec!["inter-semibold".to_string()];
    bold_stack.extend(
        fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts
        .families
        .insert(egui::FontFamily::Name("inter-semibold".into()), bold_stack);
    ctx.set_fonts(fonts);
}

pub fn apply_theme(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = SIDEBAR;
    v.faint_bg_color = Color32::from_rgb(0x1E, 0x22, 0x26);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_stroke = egui::Stroke::new(1.0, BORDER);
    let r = egui::CornerRadius::same(8);
    v.widgets.noninteractive.bg_fill = CARD;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x2B, 0x2F, 0x36);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2B, 0x2F, 0x36);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x36, 0x3B, 0x44);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x36, 0x3B, 0x44);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.5));
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
}

fn nav() -> [(View, &'static str); 7] {
    [
        (View::Home, t("Ana Sayfa", "Home")),
        (View::Scan, t("Tarama", "Scan")),
        (View::Results, t("Sonuçlar", "Results")),
        (View::Clean, t("Temizlik", "Cleanup")),
        (View::Cache, t("AI Önbelleği", "AI cache")),
        (View::Settings, t("Ayarlar", "Settings")),
        (View::About, t("Hakkında", "About")),
    ]
}

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::left("sidebar")
        .exact_size(210.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(SIDEBAR)
                .inner_margin(egui::Margin::same(14)),
        )
        .show(ui, |ui| sidebar(app, ui));

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(BG)
                .inner_margin(egui::Margin::same(22)),
        )
        .show(ui, |ui| match app.view {
            View::Home => scan_view::home(app, ui),
            View::Scan => scan_view::progress(app, ui),
            View::Results => result_view::results(app, ui),
            View::Clean => result_view::clean(app, ui),
            View::Cache => settings_view::cache(app, ui),
            View::Settings => settings_view::settings(app, ui),
            View::About => about_view::about(app, ui),
        });

    result_view::confirm_modal(app, ui.ctx());
    result_view::ai_modal(app, ui.ctx());
    toast(app, ui.ctx());
}

fn sidebar(app: &mut App, ui: &mut egui::Ui) {
    widgets::logo(ui, &app.logo);
    ui.add_space(18.0);
    for (view, label) in nav() {
        let selected = app.view == view;
        let busy = (view == View::Scan && app.scan.is_some())
            || (view == View::Clean && app.clean_job.is_some());
        let text = if busy {
            format!("{label}  •")
        } else {
            label.to_string()
        };
        let btn =
            egui::Button::new(RichText::new(text).color(if selected { ACCENT } else { TEXT }))
                .fill(if selected {
                    ACCENT_DIM
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(ui.available_width(), 36.0));
        if ui.add(btn).clicked() {
            app.view = view;
        }
    }
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .color(MUTED)
                .size(13.0),
        );
        if let Some(crate::app::UpdateState::Available(rel)) = &app.update {
            ui.hyperlink_to(
                RichText::new(draxbyte::tf!("Yeni sürüm {}", "New version {}", rel.tag)).color(ACCENT).size(13.0),
                &rel.url,
            );
        }
        let ai = if app.ai.is_some() {
            t("AI açık (her istek onaylı)", "AI on · you approve")
        } else if app.cfg.ui.check_updates {
            t("AI kapalı · yalnız güncelleme kontrolü", "AI off · update check only")
        } else {
            t("AI kapalı · ağ isteği yok", "AI off · no network requests")
        };
        ui.label(
            RichText::new(ai)
                .color(if app.ai.is_some() { WARN } else { ACCENT })
                .size(13.0),
        );
        ui.label(
            RichText::new(t("• Sistem durumu", "• System status"))
                .color(ACCENT)
                .size(13.0),
        );
    });
}

fn toast(app: &mut App, ctx: &egui::Context) {
    let Some((msg, _)) = &app.toast else { return };
    egui::Area::new(egui::Id::unique("toast"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(msg.as_str());
            });
        });
}
