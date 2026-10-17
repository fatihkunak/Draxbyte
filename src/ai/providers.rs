//! Provider registry (OpenAI-compatible servers + Anthropic), credential resolution,
//! model-list fetching and the actual chat request.
//!
//! The registry mirrors the provider list the author uses in another project, plus two
//! local servers (Ollama, LM Studio) that cost nothing. `default_model` is only a starting
//! point: model ids age quickly, so the UI fetches the real list from the provider.

use crate::ai::error::AiError;
use crate::ai::prompt::{Prompt, MAX_TOKENS};
use crate::config::AiConfig;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    OpenAiCompat,
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// Runs on your machine: no key, no cost.
    Local,
    FreeTier,
    Paid,
    Other,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Local => "Yerel (ücretsiz, anahtarsız)",
            Group::FreeTier => "Ücretsiz katmanı olanlar",
            Group::Paid => "Ücretli",
            Group::Other => "Diğer",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub group: Group,
    /// `None` → the user types it (custom / Alibaba workspace URL).
    pub base_url: Option<&'static str>,
    /// `None` → the user must pick a model (fetch the list).
    pub default_model: Option<&'static str>,
    pub key_url: Option<&'static str>,
    /// Newer OpenAI models want `max_completion_tokens`.
    pub max_tokens_param: &'static str,
    /// Auth header is `X-API-Key` instead of `Authorization: Bearer` (EVREN).
    pub x_api_key: bool,
    pub needs_key: bool,
}

const fn p(
    id: &'static str,
    label: &'static str,
    kind: Kind,
    group: Group,
    base_url: Option<&'static str>,
    default_model: Option<&'static str>,
    key_url: Option<&'static str>,
) -> ProviderInfo {
    ProviderInfo {
        id,
        label,
        kind,
        group,
        base_url,
        default_model,
        key_url,
        max_tokens_param: "max_tokens",
        x_api_key: false,
        needs_key: !matches!(group, Group::Local),
    }
}

use Group::*;
use Kind::*;

pub static PROVIDERS: &[ProviderInfo] = &[
    p(
        "ollama",
        "Ollama (yerel)",
        OpenAiCompat,
        Local,
        Some("http://localhost:11434/v1"),
        None,
        Some("https://ollama.com/download"),
    ),
    p(
        "lmstudio",
        "LM Studio (yerel)",
        OpenAiCompat,
        Local,
        Some("http://localhost:1234/v1"),
        None,
        Some("https://lmstudio.ai"),
    ),
    p(
        "gemini",
        "Google Gemini",
        OpenAiCompat,
        FreeTier,
        Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        Some("gemini-3.8-flash"),
        Some("https://aistudio.google.com/apikey"),
    ),
    p(
        "groq",
        "Groq",
        OpenAiCompat,
        FreeTier,
        Some("https://api.groq.com/openai/v1"),
        Some("qwen/qwen3.8-27b"),
        Some("https://console.groq.com/keys"),
    ),
    p(
        "openrouter",
        "OpenRouter",
        OpenAiCompat,
        FreeTier,
        Some("https://openrouter.ai/api/v1"),
        Some("openrouter/free"),
        Some("https://openrouter.ai/keys"),
    ),
    p(
        "nvidia",
        "NVIDIA NIM",
        OpenAiCompat,
        FreeTier,
        Some("https://integrate.api.nvidia.com/v1"),
        None,
        Some("https://build.nvidia.com"),
    ),
    p(
        "mistral",
        "Mistral",
        OpenAiCompat,
        FreeTier,
        Some("https://api.mistral.ai/v1"),
        Some("mistral-small-latest"),
        Some("https://console.mistral.ai/api-keys"),
    ),
    p(
        "anthropic",
        "Anthropic Claude",
        Anthropic,
        Paid,
        Some("https://api.anthropic.com/v1"),
        Some("claude-haiku-5-5"),
        Some("https://console.anthropic.com/settings/keys"),
    ),
    ProviderInfo {
        max_tokens_param: "max_completion_tokens",
        ..p(
            "openai",
            "OpenAI",
            OpenAiCompat,
            Paid,
            Some("https://api.openai.com/v1"),
            Some("gpt-4o-mini"),
            Some("https://platform.openai.com/api-keys"),
        )
    },
    p(
        "kimi",
        "Kimi (Moonshot)",
        OpenAiCompat,
        Paid,
        Some("https://api.moonshot.ai/v1"),
        Some("kimi-k2.6"),
        Some("https://platform.moonshot.ai/console/api-keys"),
    ),
    p(
        "xai",
        "xAI Grok",
        OpenAiCompat,
        Paid,
        Some("https://api.x.ai/v1"),
        None,
        Some("https://console.x.ai"),
    ),
    ProviderInfo {
        x_api_key: true,
        ..p(
            "evren",
            "EVREN LLM",
            OpenAiCompat,
            Other,
            Some("https://evren-llmapi.ssyz.org.tr/v1"),
            None,
            Some("https://evren.ssyz.org.tr/llm-inference/"),
        )
    },
    // The endpoint is specific to the workspace (`https://ws-<id>.<region>.maas.aliyuncs.com/compatible-mode/v1`).
    p(
        "alibaba",
        "Alibaba Qwen (Model Studio)",
        OpenAiCompat,
        Other,
        None,
        None,
        Some("https://modelstudio.console.alibabacloud.com/"),
    ),
    p(
        "custom",
        "OpenAI uyumlu (özel adres)",
        OpenAiCompat,
        Other,
        None,
        None,
        None,
    ),
];

pub fn get(id: &str) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|p| p.id == id)
}

impl ProviderInfo {
    /// The user may override/enter the address for these.
    pub fn base_url_editable(&self) -> bool {
        self.base_url.is_none() || self.group == Group::Local
    }
}

/// Everything needed for one request, resolved from the config.
#[derive(Debug, Clone)]
pub struct Creds {
    pub info: &'static ProviderInfo,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") || host.ends_with(".localhost")
}

/// Keys must never travel in clear text: https, except for loopback and local-group servers.
fn validate_base_url(raw: &str, info: &ProviderInfo) -> Result<(), AiError> {
    let u = reqwest::Url::parse(raw)
        .map_err(|_| AiError::BadBaseUrl("geçerli bir URL değil".into()))?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err(AiError::BadBaseUrl(
            "URL içinde kullanıcı/parola olamaz".into(),
        ));
    }
    let host = u.host_str().unwrap_or("");
    match u.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(host) || info.group == Group::Local => Ok(()),
        "http" => Err(AiError::BadBaseUrl(
            "http yalnızca yerel sunucular için; https kullan".into(),
        )),
        other => Err(AiError::BadBaseUrl(format!("desteklenmeyen şema: {other}"))),
    }
}

/// Turns the `[ai]` config into usable credentials. `model` may be empty only for
/// providers without a default; the caller must then have fetched/typed one.
pub fn resolve(cfg: &AiConfig) -> Result<Creds, AiError> {
    let info = get(&cfg.provider).ok_or_else(|| AiError::BadProvider(cfg.provider.clone()))?;
    let typed = cfg.base_url.trim().trim_end_matches('/');
    // A leftover address that belongs to another provider (older config files stored
    // the OpenAI URL here) is not an override.
    let is_override = !typed.is_empty() && !PROVIDERS.iter().any(|p| p.base_url == Some(typed));
    let base_url = match info.base_url {
        Some(fixed) if !(info.group == Group::Local && is_override) => fixed.to_string(),
        _ => typed.to_string(),
    };
    if base_url.is_empty() {
        return Err(AiError::BadBaseUrl(
            "bu sağlayıcı için adres gerekli".into(),
        ));
    }
    validate_base_url(&base_url, info)?;
    let model = Some(cfg.model.trim())
        .filter(|m| !m.is_empty())
        .or(info.default_model)
        .unwrap_or("")
        .to_string();
    let api_key = cfg.resolved_api_key().unwrap_or_default();
    if info.needs_key && api_key.is_empty() {
        return Err(AiError::NoApiKey);
    }
    Ok(Creds {
        info,
        api_key,
        model,
        base_url,
    })
}

pub fn auth_headers(c: &Creds) -> Vec<(&'static str, String)> {
    if c.api_key.is_empty() {
        return Vec::new();
    }
    match c.info.kind {
        Kind::Anthropic => vec![
            ("x-api-key", c.api_key.clone()),
            ("anthropic-version", "2023-06-01".into()),
        ],
        Kind::OpenAiCompat if c.info.x_api_key => vec![("x-api-key", c.api_key.clone())],
        Kind::OpenAiCompat => vec![("authorization", format!("Bearer {}", c.api_key))],
    }
}

pub fn chat_url(c: &Creds) -> String {
    match c.info.kind {
        Kind::Anthropic => format!("{}/messages", c.base_url),
        Kind::OpenAiCompat => format!("{}/chat/completions", c.base_url),
    }
}

pub fn request_body(c: &Creds, p: &Prompt) -> Value {
    match c.info.kind {
        Kind::Anthropic => json!({
            "model": c.model,
            "max_tokens": MAX_TOKENS,
            "system": p.system,
            "messages": [{ "role": "user", "content": p.user }],
        }),
        Kind::OpenAiCompat => json!({
            "model": c.model,
            c.info.max_tokens_param: MAX_TOKENS,
            "stream": false,
            "messages": [
                { "role": "system", "content": p.system },
                { "role": "user", "content": p.user }
            ],
        }),
    }
}

/// Returns (answer text, tokens used). Falls back to `estimate` when the server does
/// not report usage (some local servers don't).
pub fn parse_response(kind: Kind, v: &Value, estimate: u64) -> Result<(String, u64), AiError> {
    let (text, tokens) = match kind {
        Kind::Anthropic => (
            v.pointer("/content/0/text").and_then(Value::as_str),
            v.pointer("/usage").map(|u| {
                u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0)
                    + u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0)
            }),
        ),
        Kind::OpenAiCompat => (
            v.pointer("/choices/0/message/content")
                .and_then(Value::as_str),
            v.pointer("/usage/total_tokens").and_then(Value::as_u64),
        ),
    };
    let text = text
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AiError::BadResponse(v.to_string().chars().take(200).collect()))?;
    Ok((text, tokens.filter(|t| *t > 0).unwrap_or(estimate)))
}

/// Model ids from a `GET /models` body. Handles `{data:[{id}]}` (OpenAI & most),
/// `{models:[{name:"models/x"}]}` (Gemini-style) and `{models:[{id}]}`.
pub fn parse_model_list(v: &Value) -> Vec<String> {
    let arr = v
        .get("data")
        .or_else(|| v.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids: Vec<String> = arr
        .iter()
        .filter_map(|m| {
            m.get("id")
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)
        })
        .map(|s| s.strip_prefix("models/").unwrap_or(s).to_string())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .collect();
    ids.sort();
    ids.dedup();
    ids.truncate(300);
    ids
}

/// Provider error bodies can echo the key; never show it.
fn redact(text: &str, key: &str) -> String {
    let t = if key.len() >= 8 {
        text.replace(key, "***")
    } else {
        text.to_string()
    };
    t.chars().take(200).collect()
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

fn with_headers(mut rb: reqwest::RequestBuilder, c: &Creds) -> reqwest::RequestBuilder {
    for (k, v) in auth_headers(c) {
        rb = rb.header(k, v);
    }
    rb
}

async fn read(resp: reqwest::Response, key: &str) -> Result<Value, AiError> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AiError::Http(redact(&e.to_string(), key)))?;
    if !status.is_success() {
        return Err(AiError::Http(format!("{status}: {}", redact(&text, key))));
    }
    serde_json::from_str(&text).map_err(|_| AiError::BadResponse(redact(&text, key)))
}

/// One guarded chat request (the caller already did the cache/limit checks).
pub async fn send_chat(c: &Creds, p: &Prompt, estimate: u64) -> Result<(String, u64), AiError> {
    if c.model.is_empty() {
        return Err(AiError::ModelRequired);
    }
    let rb = client().post(chat_url(c)).json(&request_body(c, p));
    let resp = with_headers(rb, c)
        .send()
        .await
        .map_err(|e| AiError::Http(redact(&e.to_string(), &c.api_key)))?;
    parse_response(c.info.kind, &read(resp, &c.api_key).await?, estimate)
}

/// `GET {base}/models`. Costs no tokens.
pub async fn list_models(c: &Creds) -> Result<Vec<String>, AiError> {
    let url = match c.info.kind {
        Kind::Anthropic => format!("{}/models?limit=100", c.base_url),
        Kind::OpenAiCompat => format!("{}/models", c.base_url),
    };
    let resp = with_headers(client().get(url), c)
        .send()
        .await
        .map_err(|e| AiError::Http(redact(&e.to_string(), &c.api_key)))?;
    Ok(parse_model_list(&read(resp, &c.api_key).await?))
}

/// A tiny real request (key + model check). Returns (milliseconds, tokens used).
pub async fn check(c: &Creds) -> Result<(u128, u64), AiError> {
    let t0 = std::time::Instant::now();
    let probe = Prompt {
        system: "Reply with the single word OK.".into(),
        user: "ping".into(),
    };
    let (_, tokens) = send_chat(c, &probe, 30).await?;
    Ok((t0.elapsed().as_millis(), tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn cfg(provider: &str) -> AiConfig {
        AiConfig {
            provider: provider.into(),
            api_key: "sk-test-12345678".into(),
            ..AiConfig::default()
        }
    }

    #[test]
    fn registry_ids_are_unique_and_urls_are_https_or_local() {
        let mut ids: Vec<_> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
        for p in PROVIDERS {
            if let Some(u) = p.base_url {
                assert!(
                    u.starts_with("https://")
                        || (p.group == Group::Local && u.starts_with("http://localhost")),
                    "{}",
                    p.id
                );
            }
            assert_eq!(p.needs_key, p.group != Group::Local, "{}", p.id);
        }
        assert!(get("custom").is_some() && get("nope").is_none());
    }

    #[test]
    fn fixed_provider_ignores_stray_base_url_and_uses_default_model() {
        let c = resolve(&AiConfig {
            base_url: "https://evil.example".into(),
            ..cfg("openai")
        })
        .unwrap();
        assert_eq!(c.base_url, "https://api.openai.com/v1");
        let c = resolve(&AiConfig {
            model: String::new(),
            ..cfg("mistral")
        })
        .unwrap();
        assert_eq!(c.model, "mistral-small-latest");
    }

    #[test]
    fn stale_foreign_address_is_not_an_override_for_local() {
        let c = resolve(&AiConfig {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".into(),
            ..cfg("ollama")
        })
        .unwrap();
        assert_eq!(c.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn custom_needs_url_and_https_unless_loopback() {
        assert!(matches!(
            resolve(&cfg("custom")),
            Err(AiError::BadBaseUrl(_))
        ));
        let http = AiConfig {
            base_url: "http://example.com/v1".into(),
            ..cfg("custom")
        };
        assert!(matches!(resolve(&http), Err(AiError::BadBaseUrl(_))));
        let loop_ = AiConfig {
            base_url: "http://127.0.0.1:8080/v1/".into(),
            model: "m".into(),
            ..cfg("custom")
        };
        assert_eq!(
            resolve(&loop_).unwrap().base_url,
            "http://127.0.0.1:8080/v1"
        );
        let creds_in_url = AiConfig {
            base_url: "https://u:p@example.com/v1".into(),
            ..cfg("custom")
        };
        assert!(resolve(&creds_in_url).is_err());
    }

    #[test]
    fn local_providers_need_no_key_and_may_point_to_another_host() {
        let c = resolve(&AiConfig {
            api_key: String::new(),
            ..cfg("ollama")
        })
        .unwrap();
        assert_eq!(c.base_url, "http://localhost:11434/v1");
        let lan = AiConfig {
            api_key: String::new(),
            base_url: "http://192.168.1.20:11434/v1".into(),
            ..cfg("ollama")
        };
        assert_eq!(
            resolve(&lan).unwrap().base_url,
            "http://192.168.1.20:11434/v1"
        );
    }

    #[test]
    fn paid_provider_without_key_is_refused() {
        std::env::remove_var(crate::config::API_KEY_ENV);
        assert!(matches!(
            resolve(&AiConfig {
                api_key: String::new(),
                ..cfg("openai")
            }),
            Err(AiError::NoApiKey)
        ));
    }

    #[test]
    fn per_provider_key_map_is_used() {
        std::env::remove_var(crate::config::API_KEY_ENV);
        let mut c = AiConfig {
            api_key: String::new(),
            ..cfg("groq")
        };
        c.api_keys.insert("groq".into(), "gsk_groqkey123".into());
        c.api_keys.insert("openai".into(), "sk-other-key-1".into());
        assert_eq!(resolve(&c).unwrap().api_key, "gsk_groqkey123");
    }

    #[test]
    fn auth_headers_per_provider() {
        let h = |id: &str| auth_headers(&resolve(&cfg(id)).unwrap());
        assert_eq!(h("openai")[0].0, "authorization");
        assert_eq!(h("evren")[0].0, "x-api-key");
        let a = h("anthropic");
        assert_eq!((a[0].0, a[1].0), ("x-api-key", "anthropic-version"));
    }

    #[test]
    fn request_bodies_follow_each_apis_shape() {
        let prompt = Prompt {
            system: "S".into(),
            user: "U".into(),
        };
        let o = request_body(&resolve(&cfg("openai")).unwrap(), &prompt);
        assert_eq!(o["max_completion_tokens"], 200);
        assert!(o.get("max_tokens").is_none());
        assert_eq!(o["stream"], false);
        let g = request_body(&resolve(&cfg("groq")).unwrap(), &prompt);
        assert_eq!(g["max_tokens"], 200);
        let a = request_body(&resolve(&cfg("anthropic")).unwrap(), &prompt);
        assert_eq!(a["system"], "S");
        assert_eq!(a["messages"][0]["content"], "U");
        assert_eq!(
            chat_url(&resolve(&cfg("anthropic")).unwrap()),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn parse_response_both_shapes() {
        let o =
            json!({"choices":[{"message":{"content":" SAFE: tmp "}}],"usage":{"total_tokens":77}});
        assert_eq!(
            parse_response(Kind::OpenAiCompat, &o, 300).unwrap(),
            ("SAFE: tmp".to_string(), 77)
        );
        let a = json!({"content":[{"type":"text","text":"KEEP"}],"usage":{"input_tokens":40,"output_tokens":9}});
        assert_eq!(
            parse_response(Kind::Anthropic, &a, 300).unwrap(),
            ("KEEP".to_string(), 49)
        );
        let none = json!({"choices":[{"message":{"content":"x"}}]});
        assert_eq!(
            parse_response(Kind::OpenAiCompat, &none, 300).unwrap().1,
            300
        );
        assert!(parse_response(Kind::OpenAiCompat, &json!({"choices":[]}), 1).is_err());
    }

    #[test]
    fn model_list_shapes() {
        let openai = json!({"data":[{"id":"b"},{"id":"a"},{"id":"a"}]});
        assert_eq!(parse_model_list(&openai), vec!["a", "b"]);
        let gemini = json!({"models":[{"name":"models/gemini-x"},{"name":"models/gemini-a"}]});
        assert_eq!(parse_model_list(&gemini), vec!["gemini-a", "gemini-x"]);
        assert!(parse_model_list(&json!({"error":"x"})).is_empty());
    }

    #[test]
    fn redact_hides_the_key() {
        assert_eq!(
            redact("bad key sk-test-12345678 here", "sk-test-12345678"),
            "bad key *** here"
        );
    }

    /// Minimal one-shot HTTP server: returns `body`, reports the request head it received.
    fn serve_once(body: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap();
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
            let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body);
            s.write_all(resp.as_bytes()).unwrap();
        });
        (format!("http://127.0.0.1:{}/v1", addr.port()), rx)
    }

    #[tokio::test]
    async fn list_models_hits_models_endpoint_with_the_key() {
        let (url, rx) = serve_once(r#"{"data":[{"id":"zeta"},{"id":"alpha"}]}"#);
        let c = resolve(&AiConfig {
            base_url: url,
            ..cfg("custom")
        })
        .unwrap();
        assert_eq!(list_models(&c).await.unwrap(), vec!["alpha", "zeta"]);
        let head = rx.recv().unwrap().to_lowercase();
        assert!(head.starts_with("get /v1/models "), "{head}");
        assert!(
            head.contains("authorization: bearer sk-test-12345678"),
            "{head}"
        );
    }

    #[tokio::test]
    async fn check_sends_a_small_chat_request() {
        let (url, rx) =
            serve_once(r#"{"choices":[{"message":{"content":"OK"}}],"usage":{"total_tokens":12}}"#);
        let c = resolve(&AiConfig {
            base_url: url,
            model: "m1".into(),
            ..cfg("custom")
        })
        .unwrap();
        let (_, tokens) = check(&c).await.unwrap();
        assert_eq!(tokens, 12);
        let head = rx.recv().unwrap().to_lowercase();
        assert!(head.starts_with("post /v1/chat/completions "), "{head}");
    }

    #[tokio::test]
    async fn model_required_blocks_before_network() {
        let c = resolve(&AiConfig {
            base_url: "http://127.0.0.1:1/v1".into(),
            model: String::new(),
            ..cfg("custom")
        })
        .unwrap();
        assert!(matches!(
            send_chat(
                &c,
                &Prompt {
                    system: "s".into(),
                    user: "u".into()
                },
                10
            )
            .await,
            Err(AiError::ModelRequired)
        ));
    }
}
