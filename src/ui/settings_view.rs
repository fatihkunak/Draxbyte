//! Settings (edits `config.toml`) and AI cache management.

use super::widgets::{card, danger_button, heading, primary_button, stat};
use super::{ACCENT, DANGER, MUTED, WARN};
use crate::app::App;
use draxbyte::ai::providers::{self, Group};
use draxbyte::config::{Config, API_KEY_ENV};
use draxbyte::i18n::t;
use draxbyte::models::human_bytes;
use draxbyte::tf;
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
        t("Ayarlar", "Settings"),
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
                ui.label(RichText::new("Dil / Language").strong());
                let cur = app.cfg_draft.ui.language.clone();
                let label = |code: &str| match code {
                    "tr" => "Türkçe",
                    "en" => "English",
                    _ => "Auto / Otomatik",
                };
                egui::ComboBox::from_id_salt("lang_combo")
                    .width(220.0)
                    .selected_text(label(&cur))
                    .show_ui(ui, |ui| {
                        for code in ["auto", "tr", "en"] {
                            if ui.selectable_label(cur == code, label(code)).clicked() {
                                app.cfg_draft.ui.language = code.to_string();
                                // applies immediately (preview); saved with "Save"
                                draxbyte::i18n::set(draxbyte::i18n::resolve(code));
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            if cfg!(windows) {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.checkbox(
                        &mut app.cfg_draft.ui.always_admin,
                        t(
                            "Açılışta yönetici izni iste (bir UAC onayı; hiçbir klasör erişilemez kalmaz) — varsayılan açık",
                            "Ask for administrator rights at start (one UAC prompt; no folder stays out of reach) — on by default",
                        ),
                    );
                    ui.label(
                        RichText::new(t(
                            "Onayı reddedersen uygulama yine açılır; tarama bittiğinde erişilemeyen yer kalırsa tek tıkla yönetici olarak yeniden açabilirsin.",
                            "If you decline the prompt the app still opens; if a scan could not read something you can restart as administrator with one click.",
                        ))
                        .color(MUTED)
                        .small(),
                    );
                });
                ui.add_space(8.0);
            }
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.checkbox(
                    &mut app.cfg_draft.ui.check_updates,
                    t(
                        "Açılışta yeni sürüm var mı diye bak (GitHub'a tek istek; varsayılan kapalı)",
                        "Check for a new version at start-up (one request to GitHub; default: off)",
                    ),
                );
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t("Tarama", "Scan")).strong());
                ui.label(
                    RichText::new(t(
                        "Varsayılan yollar (satır başına bir tane)",
                        "Default paths (one per line)",
                    ))
                    .color(MUTED),
                );
                let init = lines(&app.cfg_draft.scan.default_paths);
                text_field(ui, T_PATHS, || init, 3);
                ui.label(
                    RichText::new(t("Hariç tutulan desenler", "Excluded patterns")).color(MUTED),
                );
                let init = lines(&app.cfg_draft.scan.exclude_patterns);
                text_field(ui, T_EXCL, || init, 3);
                ui.checkbox(&mut app.cfg_draft.scan.all_drives, t("Her dahili sürücüyü tara (yalnız varsayılan yollarla)", "Scan every internal drive (with the default paths only)"));
                ui.checkbox(&mut app.cfg_draft.scan.include_system, t("Sistem klasörlerini tara: Windows Temp, Package Cache, Windows.old… (yalnız varsayılan yollarla)", "Scan the system folders: Windows Temp, Package Cache, Windows.old… (with the default paths only)"));
                ui.horizontal(|ui| {
                    ui.label(t("Maks. derinlik", "Max depth"));
                    ui.add(egui::DragValue::new(&mut app.cfg_draft.scan.max_depth).range(1..=64));
                    ui.label(t("Min. boyut (MB)", "Min. size (MB)"));
                    ui.add(
                        egui::DragValue::new(&mut app.cfg_draft.scan.min_size_mb)
                            .range(0..=100_000),
                    );
                });
            });
            ui.add_space(8.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t("Kurallar", "Rules")).strong());
                ui.label(
                    RichText::new(t(
                        "Güvenli uzantılar (virgülle)",
                        "Safe extensions (comma-separated)",
                    ))
                    .color(MUTED),
                );
                let init = app.cfg_draft.rules.safe_extensions.join(", ");
                text_field(ui, T_EXTS, || init, 2);
                ui.horizontal(|ui| {
                    ui.label(t("Güvenli yaş (gün)", "Safe age (days)"));
                    ui.add(
                        egui::DragValue::new(&mut app.cfg_draft.rules.safe_age_days)
                            .range(1..=3650),
                    );
                    ui.label(t("Büyük dosya eşiği (MB)", "Large file threshold (MB)"));
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
                ui.label(RichText::new(t("Temizlik", "Cleanup")).strong());
                ui.checkbox(
                    &mut app.cfg_draft.cleanup.use_trash,
                    t(
                        "Varsayılan: Çöp Kutusu'na taşı",
                        "Default: move to Recycle Bin",
                    ),
                );
                ui.checkbox(
                    &mut app.cfg_draft.cleanup.confirm_permanent_delete,
                    t(
                        "Kalıcı silmede “SIL” yazdır",
                        "Require typing “DELETE” for permanent delete",
                    ),
                );
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(t("Kaydet", "Save"))).clicked() {
                    save = true;
                }
                if ui
                    .button(t("Varsayılana dön", "Reset to defaults"))
                    .clicked()
                {
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
                draxbyte::i18n::set(draxbyte::i18n::resolve(&app.cfg.ui.language));
                app.scan_paths_text = app.cfg.scan.default_paths.join("; ");
                app.rebuild_ai();
                reset_drafts(ui);
                match &app.ai_error {
                    Some(e) => {
                        let e = e.clone();
                        app.toast(tf!(
                            "Kaydedildi, ama AI açılamadı: {e}",
                            "Saved, but AI could not start: {e}"
                        ))
                    }
                    None => app.toast(t("Ayarlar kaydedildi", "Settings saved")),
                }
            }
            Err(e) => app.toast(tf!("Kaydedilemedi: {e}", "Could not save: {e}")),
        }
    }
}

pub fn cache(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("AI önbelleği", "AI cache"),
        t(
            "Aynı dosya için ikinci kez istek atılmaz (anahtar: yol + boyut + değişme zamanı).",
            "The same file is never sent twice (key: path + size + modified time).",
        ),
    );
    let Some(svc) = app.ai.clone() else {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(t(
                "AI kapalı; önbellek kullanılmıyor.",
                "AI is off; the cache is not in use.",
            ));
            if let Some(e) = &app.ai_error {
                ui.label(RichText::new(e).color(DANGER));
            }
            ui.label(
                RichText::new(t(
                    "Ayarlar → AI analizi bölümünden açabilirsin.",
                    "You can turn it on in Settings → AI analysis.",
                ))
                .color(MUTED),
            );
        });
        return;
    };
    let stats = svc.stats().unwrap_or_default();
    let used = svc.tokens_today().unwrap_or(0);
    let limit = svc.daily_limit();

    ui.horizontal_wrapped(|ui| {
        stat(
            ui,
            t("Kayıtlı yanıt", "Stored answers"),
            &stats.entries.to_string(),
            ACCENT,
        );
        stat(
            ui,
            t("Önbellek boyutu", "Cache size"),
            &human_bytes(stats.bytes),
            ACCENT,
        );
        let rate = stats
            .hit_rate()
            .map(|r| tf!("%{:.0}", "{:.0}%", r * 100.0))
            .unwrap_or_else(|| "—".into());
        stat(
            ui,
            t("İsabet oranı", "Hit rate"),
            &format!("{rate}  ({} / {})", stats.hits, stats.hits + stats.misses),
            ACCENT,
        );
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("Bugünkü token kullanımı", "Token usage today")).strong());
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
            ui.label(
                RichText::new(t(
                    "Limit doldu: AI istekleri bugün devre dışı.",
                    "Limit reached: AI requests are off for today.",
                ))
                .color(DANGER),
            );
        }
    });
    ui.add_space(10.0);
    if app.confirm_clear_cache {
        ui.label(
            RichText::new(t(
                "Tüm kayıtlı AI yanıtları silinecek (token sayacı sıfırlanmaz).",
                "All stored AI answers will be deleted (the token counter is not reset).",
            ))
            .color(WARN),
        );
        ui.horizontal(|ui| {
            if ui
                .add(danger_button(t("Evet, temizle", "Yes, clear")))
                .clicked()
            {
                match svc.clear_cache() {
                    Ok(()) => app.toast(t("Önbellek temizlendi", "Cache cleared")),
                    Err(e) => app.toast(e.to_string()),
                }
                app.confirm_clear_cache = false;
            }
            if ui.button(t("Vazgeç", "Cancel")).clicked() {
                app.confirm_clear_cache = false;
            }
        });
    } else if ui.button(t("Önbelleği temizle", "Clear cache")).clicked() {
        app.confirm_clear_cache = true;
    }
}

fn ai_section(app: &mut App, ui: &mut egui::Ui, want_fetch: &mut bool, want_test: &mut bool) {
    ui.label(RichText::new(t("AI analizi (isteğe bağlı)", "AI analysis (optional)")).strong());
    ui.checkbox(
        &mut app.cfg_draft.ai.enabled,
        t(
            "AI analizini aç (varsayılan: kapalı — kapalıyken hiç ağ isteği atılmaz)",
            "Enable AI analysis (default: off — no network request is made while off)",
        ),
    );
    ui.label(
        RichText::new(t(
            "Her istek tek tek onayına sunulur; yanıtlar yerel önbellekte tutulur.",
            "Every request is shown for your approval; answers are kept in a local cache.",
        ))
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
        ui.label(t("Sağlayıcı", "Provider"));
        egui::ComboBox::from_id_salt("provider_combo")
            .width(300.0)
            .selected_text(info.display_label())
            .show_ui(ui, |ui| {
                for group in [Group::Local, Group::FreeTier, Group::Paid, Group::Other] {
                    ui.label(RichText::new(group.label()).color(MUTED).small());
                    for p in providers::PROVIDERS.iter().filter(|p| p.group == group) {
                        if ui
                            .selectable_label(p.id == info.id, p.display_label())
                            .clicked()
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
            ui.label(
                RichText::new(t("sıfır token maliyeti", "zero token cost"))
                    .color(ACCENT)
                    .small(),
            );
        }
    });

    if info.base_url_editable() {
        ui.horizontal(|ui| {
            ui.label(t("Adres", "Address"));
            let hint = info.base_url.unwrap_or("https://…/v1");
            ui.add(
                egui::TextEdit::singleline(&mut app.cfg_draft.ai.base_url)
                    .hint_text(hint)
                    .desired_width(360.0),
            );
        });
    } else if let Some(u) = info.base_url {
        ui.label(
            RichText::new(tf!("Adres: {u}", "Address: {u}"))
                .color(MUTED)
                .small(),
        );
    }

    if info.needs_key {
        ui.horizontal(|ui| {
            ui.label(t("API anahtarı", "API key"));
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
                ui.hyperlink_to(t("anahtar al", "get a key"), url);
            }
        });
        let env = std::env::var(API_KEY_ENV)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let note = if env {
            tf!(
                "{API_KEY_ENV} ortam değişkeni bulundu — anahtar olarak o kullanılır.",
                "{API_KEY_ENV} environment variable found — it is used as the key."
            )
        } else {
            tf!("Anahtar config.toml'a düz metin yazılır; tercihen {API_KEY_ENV} ortam değişkenini kullan.", "The key is stored as plain text in config.toml; prefer the {API_KEY_ENV} environment variable.")
        };
        ui.label(RichText::new(note).color(MUTED).small());
    }

    ui.horizontal(|ui| {
        ui.label("Model");
        let shown = if app.cfg_draft.ai.model.is_empty() {
            t("(seç)", "(choose)").to_string()
        } else {
            app.cfg_draft.ai.model.clone()
        };
        egui::ComboBox::from_id_salt("model_combo")
            .width(300.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if app.models.is_empty() {
                    ui.label(
                        RichText::new(t(
                            "Liste boş — “Modelleri getir”e bas",
                            "List is empty — press “Fetch models”",
                        ))
                        .color(MUTED),
                    );
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
            .add_enabled(
                !busy,
                egui::Button::new(t("Modelleri getir", "Fetch models")),
            )
            .clicked()
        {
            *want_fetch = true;
        }
        if busy {
            ui.add(egui::Spinner::new());
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("ya da adını yaz", "or type the name"))
                .color(MUTED)
                .small(),
        );
        ui.add(egui::TextEdit::singleline(&mut app.cfg_draft.ai.model).desired_width(300.0));
        let busy = app.test_job.is_some();
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(t("Bağlantıyı test et", "Test connection")),
            )
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
            t("“Bağlantıyı test et” küçük bir gerçek istek atar (~30 token, günlük sayaca işlenir).", "“Test connection” sends one small real request (~30 tokens, counted in the daily total)."),
        )
        .color(MUTED)
        .small(),
    );

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("Günlük token limiti", "Daily token limit"));
        ui.add(egui::DragValue::new(&mut app.cfg_draft.ai.daily_token_limit).range(0..=10_000_000));
        ui.label(t("Önbellek yolu", "Cache path"));
        ui.add(egui::TextEdit::singleline(&mut app.cfg_draft.ai.cache_path).desired_width(220.0));
    });
    ui.checkbox(
        &mut app.cfg_draft.ai.cache_enabled,
        t(
            "Önbellek (zorunlu: kapalıysa AI çalışmaz)",
            "Cache (required: AI does not run without it)",
        ),
    );
}
