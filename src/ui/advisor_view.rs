//! AI Advisor: a chat about the last scan and a one-click report. The providers are tried in the
//! order set in Settings (failover); the scan summary that would be sent is shown first and
//! nothing is sent until the user agrees and presses send.

use super::widgets::{card, heading, primary_button};
use super::{ACCENT, ACCENT_DIM, DANGER, MUTED, WARN};
use crate::app::{App, View};
use draxbyte::ai::advisor::Role;
use draxbyte::i18n::t;
use draxbyte::tf;
use egui::RichText;

fn bubble(ui: &mut egui::Ui, fill: egui::Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(fill).corner_radius(10).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(6.0);
}

pub fn advisor(app: &mut App, ui: &mut egui::Ui) {
    heading(
        ui,
        t("AI Danışman", "AI Advisor"),
        t(
            "Tarama sonucunu konuş: ne silinebilir, ne kadar riskli, nasıl yapılır. Danışman yalnızca öneri verir, hiçbir şey silmez.",
            "Talk through the scan: what can go, how risky it is, how to do it. The advisor only advises; it never deletes anything.",
        ),
    );
    let ctx = ui.ctx().clone();

    if !app.cfg.ai.enabled {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("AI kapalı", "AI is off")).strong());
            ui.label(RichText::new(t("Varsayılan olarak kapalıdır ve kapalıyken hiç ağ isteği atılmaz. Ayarlar → AI bölümünden aç ve bir ya da birkaç sağlayıcı ekle.", "It is off by default and makes no network request while off. Turn it on in Settings → AI and add one or more providers.")).color(MUTED));
            if ui.add(primary_button(t("Ayarları aç", "Open Settings"))).clicked() {
                app.view = View::Settings;
            }
        });
        return;
    }
    let Some(adv) = app.advisor.clone() else {
        let e = app.ai_error.clone().unwrap_or_default();
        ui.label(RichText::new(tf!("Danışman başlatılamadı: {e}", "The advisor could not start: {e}")).color(WARN));
        return;
    };
    let labels = adv.chain_labels();
    if labels.is_empty() {
        ui.label(RichText::new(t("Sağlayıcı yok. Ayarlar → AI → sağlayıcı zincirine en az bir tane ekle.", "No provider. Add at least one in Settings → AI → provider chain.")).color(WARN));
        if ui.button(t("Ayarları aç", "Open Settings")).clicked() {
            app.view = View::Settings;
        }
        return;
    }
    let chain = labels.iter().enumerate().map(|(i, l)| format!("{}) {l}", i + 1)).collect::<Vec<_>>().join("  →  ");
    ui.label(RichText::new(tf!("Sırayla denenir: {}", "Tried in this order: {}", chain)).color(MUTED).small());
    ui.label(RichText::new(tf!("Bugün {} / {} token", "Today {} / {} tokens", adv.tokens_today(), adv.daily_limit())).color(MUTED).small());
    ui.add_space(6.0);

    if !app.adv.consent {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("Ne gönderilir?", "What is sent?")).strong());
            ui.label(t(
                "Yalnızca sorduğun mesajlar ve (açıksa) tarama özeti gider: boyutlar, kural adları ve kısaltılmış yollar (ev klasörü ~ olur). Dosya içeriği ASLA gitmez. Özet yukarıdaki zincirdeki sağlayıcıya gider; her istek senin 'Gönder' tuşunla başlar.",
                "Only the messages you write and (if on) the scan summary are sent: sizes, rule names and shortened paths (your home folder becomes ~). File contents NEVER leave. It goes to the first provider in the chain above; every request starts with your Send click.",
            ));
            if ui.add(primary_button(t("Anladım, sohbeti başlat", "Understood, start the chat"))).clicked() {
                app.adv.consent = true;
            }
        });
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut app.adv.include_context, t("Tarama özetini sorulara ekle", "Add the scan summary to the questions"));
        if app.rows.is_empty() {
            ui.label(RichText::new(t("(henüz tarama yok)", "(no scan yet)")).color(MUTED).small());
        }
    });
    if app.adv.include_context && !app.rows.is_empty() {
        egui::CollapsingHeader::new(RichText::new(t("Gönderilecek özeti göster", "Show the summary that is sent")).small()).show(ui, |ui| {
            let mut c = app.advisor_context();
            ui.add(egui::TextEdit::multiline(&mut c).desired_rows(6).desired_width(f32::INFINITY).font(egui::TextStyle::Small).interactive(false));
        });
    }
    ui.add_space(4.0);

    let busy = app.adv.job.is_some();
    let mut retry = false;
    egui::ScrollArea::vertical().max_height((ui.available_height() - 190.0).max(140.0)).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        if app.adv.msgs.is_empty() {
            ui.label(RichText::new(t("Bir soru yaz ya da hazır sorulardan birini seç:", "Write a question or pick one:")).color(MUTED));
            for q in [
                t("Derleme çıktısını (node_modules, target, build) silmeli miyim? Hangilerini önce silmeliyim?", "Should I delete the build output (node_modules, target, build)? Which first?"),
                t("Bu taramada en güvenli ve en çok yer kazandıran işlem nedir?", "What is the safest action that frees the most space in this scan?"),
                t("Sarı (dikkat) öğelerden hangileri risksiz?", "Which of the yellow (careful) items are low-risk?"),
            ] {
                if ui.link(q).clicked() {
                    app.adv.input = q.to_string();
                }
            }
        }
        let last = app.adv.msgs.len().saturating_sub(1);
        for (i, m) in app.adv.msgs.iter().enumerate() {
            let (fill, who) = match (m.role, m.error) {
                (_, true) => (egui::Color32::from_rgb(0x3A, 0x1C, 0x1C), t("Hata", "Error")),
                (Role::User, _) => (ACCENT_DIM, t("Sen", "You")),
                (Role::Assistant, _) => (super::CARD, t("Danışman", "Advisor")),
            };
            bubble(ui, fill, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(who).strong().color(if m.error { DANGER } else { ACCENT }));
                    if !m.meta.is_empty() {
                        ui.label(RichText::new(&m.meta).color(MUTED).small());
                    }
                });
                ui.add(egui::Label::new(m.text.as_str()).selectable(true));
                if m.role == Role::Assistant && !m.error && ui.small_button(t("Kopyala", "Copy")).clicked() {
                    ui.ctx().copy_text(m.text.clone());
                }
                if m.error && i == last && !busy && ui.small_button(t("Yeniden dene", "Try again")).clicked() {
                    retry = true;
                }
            });
        }
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(tf!("Birincil sağlayıcı deneniyor, başarısız olursa sıradakine geçilir… {} sn", "Trying the primary provider, the next one if it fails… {} s", app.adv.started.elapsed().as_secs())).color(MUTED));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    });
    if retry {
        app.advisor_retry(&ctx);
    }

    ui.add_space(4.0);
    let resp = ui.add(egui::TextEdit::multiline(&mut app.adv.input).desired_rows(3).desired_width(f32::INFINITY).hint_text(t("Sorunu yaz… (Ctrl+Enter ile gönder)", "Write your question… (Ctrl+Enter sends)")));
    let ctrl_enter = resp.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    ui.horizontal_wrapped(|ui| {
        let can = !busy && !app.adv.input.trim().is_empty();
        if (ui.add_enabled(can, primary_button(t("Gönder", "Send"))).clicked() || (ctrl_enter && can)) && can {
            let q = app.adv.input.clone();
            app.advisor_send(&ctx, &q);
        }
        if ui.add_enabled(!busy, egui::Button::new(t("AI rapor iste", "Ask for an AI report"))).on_hover_text(t("Tüm tarama için öncelik sıralı bir temizlik raporu", "A prioritized cleanup report for the whole scan")).clicked() {
            app.advisor_report(&ctx);
        }
        if ui.add_enabled(!busy && !app.adv.msgs.is_empty(), egui::Button::new(t("Sohbeti temizle", "Clear the chat"))).clicked() {
            app.adv.msgs.clear();
        }
    });
}
