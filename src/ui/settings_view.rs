//! Settings (edits `config.toml`) and AI cache management.

use super::widgets::{card, danger_button, heading, primary_button, stat};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::App;
use draxbyte::ai::providers::{self, Group};
use draxbyte::config::{Config, API_KEY_ENV};
use draxbyte::models::human_bytes;
use egui::{Id, RichText};

const T_PATHS: &str = "set_paths";
const T_EXCL: &str = "set_excl";
const T_EXTS: &str = "set_exts";

fn lines(v: &[String]) -> String {
    v.join("\n")
}

fn split_lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn split_exts(s: &str) -> Vec<String> {
    s.split([',', ';', '\n', ' '])
        .map(|e| e.trim().trim_start_matches('.').to_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

fn text_field(ui: &mut egui::Ui, key: &str, init: impl FnOnce() -> String, rows: usize) {
    let id = Id::unique(key);
    let mut s: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(init);
    ui.add(
        egui::TextEdit::multiline(&mut s)
            .desired_rows(rows)
            .desired_width(f32::INFINITY),
    );
    ui.data_mut(|d| d.insert_temp(id, s));
}

fn reset_drafts(ui: &egui::Ui) {
    for k in [T_PATHS, T_EXCL, T_EXTS] {
        ui.data_mut(|d| d.remove_temp::<String>(Id::unique(k)));
    }
}

pub fn settings(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        "Ayarlar",
        &format!("config.toml: {}", app.cfg_path.display()),
    );
    let mut save = false;
    let mut reset = false;
    let mut want_fetch = false;
    let mut want_test = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Tarama").strong());
                ui.label(RichText::new("Varsayılan yollar (satır başına bir tane)").color(MUTED));
                let init = lines(&app.cfg_draft.scan.default_paths);
                text_field(ui, T_PATHS, || init, 3);
                ui.label(RichText::new("Hariç tutulan desenler").color(MUTED));
                let init = lines(&app.cfg_draft.scan.exclude_patterns);
                text_field(ui, T_EXCL, || init, 3);
                ui.horizontal(|ui| {
                    ui.label("Maks. derinlik");
                    ui.add(egui::DragValue::new(&mut app.cfg_draft.scan.max_depth).range(1..=64));
                    ui.label("Min. boyut (MB)");
                    ui.add(
                        egui::DragValue::new(&mut app.cfg_draft.scan.min_size_mb)
                            .range(0..=100_000),
                    );
                });
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Kurallar").strong());
                ui.label(RichText::new("Güvenli uzantılar (virgülle)").color(MUTED));
                let init = app.cfg_draft.rules.safe_extensions.join(", ");
                text_field(ui, T_EXTS, || init, 2);
                ui.horizontal(|ui| {
                    ui.label("Güvenli yaş (gün)");
                    ui.add(
                        egui::DragValue::new(&mut app.cfg_draft.rules.safe_age_days)
                            .range(1..=3650),
                    );
                    ui.label("Büyük dosya eşiği (MB)");
                    ui.add(
                        egui::DragValue::new(&mut app.cfg_draft.rules.large_file_mb)
                            .range(1..=1_000_000),
                    );
                });
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ai_section(app, ui, &mut want_fetch, &mut want_test);
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Temizlik").strong());
                ui.checkbox(
                    &mut app.cfg_draft.cleanup.use_trash,
                    "Varsayılan: Çöp Kutusu'na taşı",
                );
                ui.checkbox(
                    &mut app.cfg_draft.cleanup.confirm_permanent_delete,
                    "Kalıcı silmede “SIL” yazdır",
                );
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button("Kaydet")).clicked() {
                    save = true;
                }
                if ui.button("Varsayılana dön").clicked() {
                    reset = true;
                }
            });
        });

    if want_fetch {
        let ctx = ui.ctx().clone();
        app.fetch_models(&ctx);
    }
    if want_test {
        let ctx = ui.ctx().clone();
        app.test_connection(&ctx);
    }
    if reset {
        app.cfg_draft = Config::default();
        reset_drafts(ui);
    }
    if save {
        let g = |k: &str| -> String {
            ui.data_mut(|d| d.get_temp::<String>(Id::unique(k)))
                .unwrap_or_default()
        };
        app.cfg_draft.scan.default_paths = split_lines(&g(T_PATHS));
        app.cfg_draft.scan.exclude_patterns = split_lines(&g(T_EXCL));
        app.cfg_draft.rules.safe_extensions = split_exts(&g(T_EXTS));
        match app.cfg_draft.save(&app.cfg_path) {
            Ok(()) => {
                app.cfg = app.cfg_draft.clone();
                app.scan_paths_text = app.cfg.scan.default_paths.join("; ");
                app.rebuild_ai();
                reset_drafts(ui);
                match &app.ai_error {
                    Some(e) => {
                        let e = e.clone();
                        app.toast(format!("Kaydedildi, ama AI açılamadı: {e}"))
                    }
                    None => app.toast("Ayarlar kaydedildi"),
                }
            }
            Err(e) => app.toast(format!("Kaydedilemedi: {e}")),
        }
    }
}

pub fn cache(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        "AI önbelleği",
        "Aynı dosya için ikinci kez istek atılmaz (anahtar: yol + boyut + değişme zamanı).",
    );
    let Some(svc) = app.ai.clone() else {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label("AI kapalı; önbellek kullanılmıyor.");
            if let Some(e) = &app.ai_error {
                ui.label(RichText::new(e).color(DANGER));
            }
            ui.label(RichText::new("Ayarlar → AI analizi bölümünden açabilirsin.").color(MUTED));
        });
        return;
    };
    let stats = svc.stats().unwrap_or_default();
    let used = svc.tokens_today().unwrap_or(0);
    let limit = svc.daily_limit();

    ui.horizontal_wrapped(|ui| {
        stat(ui, "Kayıtlı yanıt", &stats.entries.to_string(), ACCENT);
        stat(ui, "Önbellek boyutu", &human_bytes(stats.bytes), ACCENT);
        let rate = stats
            .hit_rate()
            .map(|r| format!("%{:.0}", r * 100.0))
            .unwrap_or_else(|| "—".into());
        stat(
            ui,
            "İsabet oranı",
            &format!("{rate}  ({} / {})", stats.hits, stats.hits + stats.misses),
            ACCENT,
        );
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Bugünkü token kullanımı").strong());
        let frac = if limit > 0 {
            used as f32 / limit as f32
        } else {
            1.0
        };
        ui.add(
            egui::ProgressBar::new(frac.min(1.0))
                .text(format!("{used} / {limit}"))
                .fill(if frac >= 1.0 {
                    DANGER
                } else if frac > 0.8 {
                    WARN
                } else {
                    ACCENT
                }),
        );
        if used >= limit {
            ui.label(RichText::new("Limit doldu: AI istekleri bugün devre dışı.").color(DANGER));
        }
    });
    ui.add_space(10.0);
    if app.confirm_clear_cache {
        ui.label(
            RichText::new("Tüm kayıtlı AI yanıtları silinecek (token sayacı sıfırlanmaz).")
                .color(WARN),
        );
        ui.horizontal(|ui| {
            if ui.add(danger_button("Evet, temizle")).clicked() {
                match svc.clear_cache() {
                    Ok(()) => app.toast("Önbellek temizlendi"),
                    Err(e) => app.toast(e.to_string()),
                }
                app.confirm_clear_cache = false;
            }
            if ui.button("Vazgeç").clicked() {
                app.confirm_clear_cache = false;
            }
        });
    } else if ui.button("Önbelleği temizle").clicked() {
        app.confirm_clear_cache = true;
    }
}

fn ai_section(app: &mut App, ui: &mut egui::Ui, want_fetch: &mut bool, want_test: &mut bool) {
    ui.label(RichText::new("AI analizi (isteğe bağlı)").strong());
    ui.checkbox(
        &mut app.cfg_draft.ai.enabled,
        "AI analizini aç (varsayılan: kapalı — kapalıyken hiç ağ isteği atılmaz)",
    );
    ui.label(
        RichText::new("Her istek tek tek onayına sunulur; yanıtlar yerel önbellekte tutulur.")
            .color(MUTED)
            .small(),
    );
    if !app.cfg_draft.ai.enabled {
        return;
    }

    // A fetched model list belongs to one provider/address.
    if app.models_job.is_none() && app.models_for != app.provider_key() {
        app.models.clear();
    }
    let info = providers::get(&app.cfg_draft.ai.provider).unwrap_or(&providers::PROVIDERS[0]);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Sağlayıcı");
        egui::ComboBox::from_id_salt("provider_combo")
            .width(300.0)
            .selected_text(info.label)
            .show_ui(ui, |ui| {
                for group in [Group::Local, Group::FreeTier, Group::Paid, Group::Other] {
                    ui.label(RichText::new(group.label()).color(MUTED).small());
                    for p in providers::PROVIDERS.iter().filter(|p| p.group == group) {
                        if ui.selectable_label(p.id == info.id, p.label).clicked()
                            && p.id != info.id
                        {
                            let ai = &mut app.cfg_draft.ai;
                            ai.provider = p.id.to_string();
                            ai.model = p.default_model.unwrap_or("").to_string();
                            ai.base_url.clear();
                            app.models.clear();
                            app.test_status = None;
                        }
                    }
                    ui.separator();
                }
            });
        if info.group == Group::Local {
            ui.label(RichText::new("sıfır token maliyeti").color(ACCENT).small());
        }
    });

    if info.base_url_editable() {
        ui.horizontal(|ui| {
            ui.label("Adres");
            let hint = info.base_url.unwrap_or("https://…/v1");
            ui.add(
                egui::TextEdit::singleline(&mut app.cfg_draft.ai.base_url)
                    .hint_text(hint)
                    .desired_width(360.0),
            );
        });
    } else if let Some(u) = info.base_url {
        ui.label(RichText::new(format!("Adres: {u}")).color(MUTED).small());
    }

    if info.needs_key {
        ui.horizontal(|ui| {
            ui.label("API anahtarı");
            let key = app
                .cfg_draft
                .ai
                .api_keys
                .entry(info.id.to_string())
                .or_default();
            ui.add(
                egui::TextEdit::singleline(key)
                    .password(true)
                    .desired_width(300.0),
            );
            if let Some(url) = info.key_url {
                ui.hyperlink_to("anahtar al", url);
            }
        });
        let env = std::env::var(API_KEY_ENV)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let note = if env {
            format!("{API_KEY_ENV} ortam değişkeni bulundu — anahtar olarak o kullanılır.")
        } else {
            format!("Anahtar config.toml'a düz metin yazılır; tercihen {API_KEY_ENV} ortam değişkenini kullan.")
        };
        ui.label(RichText::new(note).color(MUTED).small());
    }

    ui.horizontal(|ui| {
        ui.label("Model");
        let shown = if app.cfg_draft.ai.model.is_empty() {
            "(seç)".to_string()
        } else {
            app.cfg_draft.ai.model.clone()
        };
        egui::ComboBox::from_id_salt("model_combo")
            .width(300.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if app.models.is_empty() {
                    ui.label(RichText::new("Liste boş — “Modelleri getir”e bas").color(MUTED));
                }
                ui.add(egui::TextEdit::singleline(&mut app.model_filter).hint_text("ara"));
                let f = app.model_filter.to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for m in app
                            .models
                            .iter()
                            .filter(|m| f.is_empty() || m.to_lowercase().contains(&f))
                        {
                            if ui
                                .selectable_label(*m == app.cfg_draft.ai.model, m)
                                .clicked()
                            {
                                app.cfg_draft.ai.model = m.clone();
                            }
                        }
                    });
            });
        let busy = app.models_job.is_some();
        if ui
            .add_enabled(!busy, egui::Button::new("Modelleri getir"))
            .clicked()
        {
            *want_fetch = true;
        }
        if busy {
            ui.add(egui::Spinner::new());
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("ya da adını yaz").color(MUTED).small());
        ui.add(egui::TextEdit::singleline(&mut app.cfg_draft.ai.model).desired_width(300.0));
        let busy = app.test_job.is_some();
        if ui
            .add_enabled(!busy, egui::Button::new("Bağlantıyı test et"))
            .clicked()
        {
            *want_test = true;
        }
        if busy {
            ui.add(egui::Spinner::new());
        }
    });
    if let Some((ok, msg)) = &app.test_status {
        ui.label(
            RichText::new(if *ok {
                format!("✓ {msg}")
            } else {
                format!("✗ {msg}")
            })
            .color(if *ok { ACCENT } else { DANGER }),
        );
    }
    ui.label(
        RichText::new(
            "“Bağlantıyı test et” küçük bir gerçek istek atar (~30 token, günlük sayaca işlenir).",
        )
        .color(MUTED)
        .small(),
    );

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Günlük token limiti");
        ui.add(egui::DragValue::new(&mut app.cfg_draft.ai.daily_token_limit).range(0..=10_000_000));
        ui.label("Önbellek yolu");
        ui.add(egui::TextEdit::singleline(&mut app.cfg_draft.ai.cache_path).desired_width(220.0));
    });
    ui.checkbox(
        &mut app.cfg_draft.ai.cache_enabled,
        "Önbellek (zorunlu: kapalıysa AI çalışmaz)",
    );
}
