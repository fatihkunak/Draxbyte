//! AI Advisor: a chat about the last scan and a one-click report. The providers are tried in the
//! order set in Settings (failover); the scan summary that would be sent is shown first and
//! nothing is sent until the user agrees and presses send.

use super::widgets::{card, heading, primary_button};
use super::{bold, ACCENT, ACCENT_DIM, DANGER, MUTED, WARN};
use crate::app::{App, View};
use draxbyte::ai::advisor::Role;
use draxbyte::i18n::t;
use draxbyte::tf;
use egui::RichText;

/// One styled piece of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MdLine {
    heading: bool,
    /// Indent and bullet that start the line.
    prefix: String,
    runs: Vec<Run>,
}

/// One line of an answer: `**bold**`, `*italic*`, `` `code` ``, `#` headings, `-`/`*` bullets.
/// Models answer in Markdown; shown raw it is full of asterisks. Only the text is styled, nothing is fetched or run.
fn md_parse(line: &str) -> MdLine {
    let indent = line.len() - line.trim_start().len();
    let mut rest = line.trim_start();
    let mut out = MdLine::default();
    if rest.starts_with('#') {
        let t = rest.trim_start_matches('#');
        if t.starts_with(' ') {
            out.heading = true;
            rest = t.trim_start();
        }
    } else if let Some(r) = rest.strip_prefix("* ").or_else(|| rest.strip_prefix("- ")).or_else(|| rest.strip_prefix("• ")) {
        out.prefix = format!("{}• ", " ".repeat(indent.min(8)));
        rest = r;
    } else if indent > 0 {
        out.prefix = " ".repeat(indent.min(8));
    }
    let (mut b, mut i, mut c) = (false, false, false);
    let mut cur = String::new();
    let flush = |runs: &mut Vec<Run>, cur: &mut String, b: bool, i: bool, c: bool| {
        if !cur.is_empty() {
            runs.push(Run { text: std::mem::take(cur), bold: b, italic: i, code: c });
        }
    };
    let chars: Vec<char> = rest.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let ch = chars[k];
        let next = chars.get(k + 1).copied();
        if ch == '`' {
            flush(&mut out.runs, &mut cur, b, i, c);
            c = !c;
        } else if c {
            cur.push(ch);
        } else if ch == '*' && next == Some('*') {
            flush(&mut out.runs, &mut cur, b, i, c);
            b = !b;
            k += 1;
        } else if ch == '*' && (i || next.is_some_and(|n| !n.is_whitespace())) {
            flush(&mut out.runs, &mut cur, b, i, c);
            i = !i;
        } else {
            cur.push(ch);
        }
        k += 1;
    }
    flush(&mut out.runs, &mut cur, b, i, c);
    out
}

fn markdown_job(line: &str, width: f32, ui: &egui::Ui) -> egui::text::LayoutJob {
    use egui::{text::LayoutJob, Color32, FontId, TextFormat, TextStyle};
    let size = ui.text_style_height(&TextStyle::Body);
    let color = ui.visuals().text_color();
    let md = md_parse(line);
    let head = if md.heading { size + 2.0 } else { size };
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    if !md.prefix.is_empty() {
        job.append(&md.prefix, 0.0, TextFormat { font_id: FontId::proportional(size), color, ..Default::default() });
    }
    for r in &md.runs {
        let font = if r.code {
            FontId::monospace(size - 1.0)
        } else if r.bold || md.heading {
            bold(head)
        } else {
            FontId::proportional(size)
        };
        let mut f = TextFormat { font_id: font, color, italics: r.italic && !r.code, ..Default::default() };
        if r.code {
            f.background = Color32::from_white_alpha(18);
        }
        job.append(&r.text, 0.0, f);
    }
    job
}

/// An answer, line by line (see `markdown_job`).
fn markdown(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width();
    for line in text.lines() {
        if line.trim().is_empty() {
            ui.add_space(6.0);
        } else {
            let job = markdown_job(line, width, ui);
            ui.add(egui::Label::new(job).selectable(true));
        }
    }
}

fn bubble(ui: &mut egui::Ui, fill: egui::Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(fill).corner_radius(10).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(6.0);
}

/// Provider + model of the next question. Picking a model switches to its provider at once; the other
/// providers of the chain stay behind it as backups. Lists come from the providers (`↻`), or type a name.
fn model_picker(app: &mut App, ui: &mut egui::Ui, adv: &draxbyte::ai::advisor::Advisor, ctx: &egui::Context) {
    let targets = adv.targets();
    if app.adv.pick.index >= targets.len() {
        app.adv.pick = Default::default();
    }
    let cur = targets[app.adv.pick.index].clone();
    let cur_model = if app.adv.pick.model.trim().is_empty() { cur.model.clone() } else { app.adv.pick.model.clone() };
    let shown = draxbyte::ai::advisor::label_of(&cur.provider, &cur_model);
    let busy = app.adv.job.is_some();
    let mut fetch: Option<usize> = None;
    let mut terms = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t("Model", "Model")).strong());
        ui.add_enabled_ui(!busy, |ui| {
            egui::ComboBox::from_id_salt("advisor_model").width(330.0).selected_text(shown).show_ui(ui, |ui| {
                for (i, tg) in targets.iter().enumerate() {
                    let name = draxbyte::ai::providers::get(&tg.provider).map(|p| p.display_label()).unwrap_or(tg.provider.as_str());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).strong().color(ACCENT));
                        let loading = app.adv.models_job.as_ref().is_some_and(|(j, _)| *j == i);
                        if ui.add_enabled(!loading, egui::Button::new(t("↻ modelleri getir", "↻ fetch models")).small()).clicked() {
                            fetch = Some(i);
                        }
                        if loading {
                            ui.spinner();
                        }
                    });
                    let mut list: Vec<String> = app.adv.models.get(&i).cloned().unwrap_or_default();
                    if !tg.model.is_empty() && !list.contains(&tg.model) {
                        list.insert(0, tg.model.clone());
                    }
                    if list.is_empty() {
                        ui.label(RichText::new(t("  (model adı yaz ya da getir)", "  (type a model name or fetch)")).color(MUTED).small());
                    }
                    egui::ScrollArea::vertical().id_salt(("advisor_models", i)).max_height(160.0).show(ui, |ui| {
                        for m in list {
                            let on = app.adv.pick.index == i && cur_model == m;
                            if ui.selectable_label(on, &m).clicked() && !on {
                                // a model of the same or of another provider: that provider answers first from now on
                                let model = if m == tg.model { String::new() } else { m };
                                app.adv.pick = draxbyte::ai::advisor::Pick { index: i, model };
                            }
                        }
                    });
                    ui.separator();
                }
            });
        });
        let mut typed = cur_model.clone();
        let r = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut typed).desired_width(210.0).hint_text(t("ya da model adını yaz", "or type a model name")));
        if r.changed() {
            let name = typed.trim().to_string();
            app.adv.pick.model = if name == cur.model { String::new() } else { name };
        }
        if let Some((ok, msg)) = &app.adv.models_note {
            if !*ok && draxbyte::ai::providers::is_terms_error(msg) && ui.small_button(t("Kullanım şartlarını göster", "Show the terms of use")).clicked() {
                terms = true;
            }
            ui.label(RichText::new(if *ok { format!("✓ {msg}") } else { format!("✗ {msg}") }).color(if *ok { ACCENT } else { DANGER }).small());
        }
    });
    if terms {
        app.open_terms_advisor(ctx);
    }
    if let Some(i) = fetch {
        app.advisor_fetch_models(ctx, i);
    }
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
    model_picker(app, ui, &adv, &ctx);
    // the picked provider (with its chosen model) first, the rest as in Settings
    let mut order = labels.clone();
    if app.adv.pick.index < order.len() {
        let first = order.remove(app.adv.pick.index);
        let tg = &adv.targets()[app.adv.pick.index];
        let model = if app.adv.pick.model.trim().is_empty() { tg.model.clone() } else { app.adv.pick.model.clone() };
        order.insert(0, if app.adv.pick.model.trim().is_empty() { first } else { draxbyte::ai::advisor::label_of(&tg.provider, &model) });
    }
    let chain = order.iter().enumerate().map(|(i, l)| format!("{}) {l}", i + 1)).collect::<Vec<_>>().join("  →  ");
    ui.label(RichText::new(tf!("Yedek sırası: {}", "Backup order: {}", chain)).color(MUTED).small());
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
    let mut terms = false;
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
                if m.role == Role::Assistant && !m.error {
                    markdown(ui, &m.text);
                } else {
                    ui.add(egui::Label::new(m.text.as_str()).selectable(true));
                }
                if m.role == Role::Assistant && !m.error && ui.small_button(t("Kopyala", "Copy")).clicked() {
                    ui.ctx().copy_text(m.text.clone());
                }
                if m.error && draxbyte::ai::providers::is_terms_error(&m.text) && ui.button(t("Kullanım şartlarını göster", "Show the terms of use")).clicked() {
                    terms = true;
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
    if terms {
        app.open_terms_advisor(&ctx);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(l: &MdLine) -> String {
        l.runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn bold_italic_and_code_lose_their_markers() {
        let l = md_parse("1. **`node_modules` (612 MB):** costs *a few minutes* and `npm install`");
        assert_eq!(plain(&l), "1. node_modules (612 MB): costs a few minutes and npm install");
        assert!(l.runs.iter().any(|r| r.bold && r.code && r.text == "node_modules"), "{:?}", l.runs);
        assert!(l.runs.iter().any(|r| r.italic && r.text == "a few minutes"));
        assert!(l.runs.iter().any(|r| r.code && !r.bold && r.text == "npm install"));
    }

    #[test]
    fn bullets_headings_and_indent() {
        let b = md_parse("   * **Risk:** low");
        assert_eq!(b.prefix, "   • ");
        assert_eq!(plain(&b), "Risk: low");
        let h = md_parse("## Plan");
        assert!(h.heading && plain(&h) == "Plan");
        assert_eq!(md_parse("not # a heading").heading, false);
    }

    #[test]
    fn stray_or_literal_asterisks_do_not_swallow_text() {
        // a multiplication sign and an unmatched marker stay readable; the formatting never leaks into the next line
        assert_eq!(plain(&md_parse("3 * 4 = 12")), "3 * 4 = 12");
        let l = md_parse("**never closed");
        assert_eq!(plain(&l), "never closed");
        assert_eq!(plain(&md_parse("next line")), "next line");
        // inside code, asterisks are literal
        assert_eq!(plain(&md_parse("`a ** b`")), "a ** b");
    }
}
