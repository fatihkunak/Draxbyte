//! State and background job of the AI Advisor screen (the screen is `ui/advisor_view.rs`; the
//! providers, failover and prompt are in `draxbyte::ai::advisor`). Chats are kept in memory only.

use crate::app::App;
use draxbyte::ai::advisor::{self, AdvisorError, Reply, Role, Turn};
use draxbyte::i18n::t;
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

pub struct ChatMsg {
    pub role: Role,
    pub text: String,
    /// Who answered / how (provider, cache, failover); empty for the user's own messages.
    pub meta: String,
    pub error: bool,
}

pub struct AdvisorState {
    pub msgs: Vec<ChatMsg>,
    pub input: String,
    /// The user agreed (this session) that the scan summary goes to the chain's providers.
    pub consent: bool,
    pub include_context: bool,
    pub job: Option<Receiver<Result<Reply, AdvisorError>>>,
    pub started: Instant,
}

impl Default for AdvisorState {
    fn default() -> Self {
        Self { msgs: Vec::new(), input: String::new(), consent: false, include_context: true, job: None, started: Instant::now() }
    }
}

impl App {
    /// The scan summary exactly as it would be sent (the screen shows it before anything leaves).
    pub fn advisor_context(&self) -> String {
        advisor::build_context(self.rows.iter().map(|r| &r.item), &draxbyte::config::home_dir(), draxbyte::models::now_secs())
    }

    fn advisor_spawn(&mut self, ctx: &egui::Context, include_context: bool) {
        let Some(adv) = self.advisor.clone() else { return };
        let turns: Vec<Turn> = self.adv.msgs.iter().filter(|m| !m.error).map(|m| Turn { role: m.role, text: m.text.clone() }).collect();
        let context = if include_context { self.advisor_context() } else { String::new() };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let _ = tx.send(adv.ask(&context, &turns).await);
            ctx.request_repaint();
        });
        self.adv.job = Some(rx);
        self.adv.started = Instant::now();
    }

    /// Sends the user's message. Needs the session consent; one request, started by the user.
    pub fn advisor_send(&mut self, ctx: &egui::Context, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.adv.job.is_some() || !self.adv.consent || self.advisor.is_none() {
            return;
        }
        self.adv.msgs.push(ChatMsg { role: Role::User, text: text.to_string(), meta: String::new(), error: false });
        self.adv.input.clear();
        let inc = self.adv.include_context;
        self.advisor_spawn(ctx, inc);
    }

    /// "Ask for an AI report": a fixed request about the whole scan (always with the summary).
    pub fn advisor_report(&mut self, ctx: &egui::Context) {
        if self.rows.is_empty() {
            self.toast(t("Önce bir tarama yap: rapor tarama sonucuna dayanır", "Run a scan first: the report is based on its result"));
            return;
        }
        self.adv.include_context = true;
        let q = t(
            "Bu taramadan öncelik sırasına göre bir temizlik raporu hazırla: en çok yer kazandıracak ilk 5 işlem; her biri için tahmini kazanç, yeniden üretme maliyeti, risk ve Draxbyte'ta nasıl yapılacağı. Sonda yapmamam gerekenleri yaz.",
            "From this scan, write a cleanup report in priority order: the 5 actions that free the most space; for each the estimated gain, the cost of regenerating it, the risk and how to do it in Draxbyte. End with what I should not touch.",
        );
        self.advisor_send(ctx, q);
    }

    /// Asks again after a failure, without adding the user's message a second time.
    pub fn advisor_retry(&mut self, ctx: &egui::Context) {
        if self.adv.job.is_some() || !self.adv.consent || self.advisor.is_none() {
            return;
        }
        while self.adv.msgs.last().is_some_and(|m| m.error) {
            self.adv.msgs.pop();
        }
        if self.adv.msgs.last().is_some_and(|m| m.role == Role::User) {
            let inc = self.adv.include_context;
            self.advisor_spawn(ctx, inc);
        }
    }

    /// Called every frame from `poll_jobs`.
    pub fn advisor_poll(&mut self) {
        let Some(rx) = &self.adv.job else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.adv.job = None;
        match res {
            Ok(r) => self.adv.msgs.push(ChatMsg { role: Role::Assistant, meta: advisor::describe_reply(&r), text: r.text, error: false }),
            Err(e) => self.adv.msgs.push(ChatMsg { role: Role::Assistant, text: e.to_string(), meta: String::new(), error: true }),
        }
    }
}
