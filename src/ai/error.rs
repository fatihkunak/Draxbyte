use crate::tr;
use std::fmt;

#[derive(Debug)]
pub enum AiError {
    Disabled,
    CacheRequired,
    BadProvider(String),
    BadBaseUrl(String),
    ModelRequired,
    NoApiKey,
    LimitReached { used: u64, limit: u64 },
    Cache(String),
    Http(String),
    BadResponse(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AiError::Disabled => tr!(
                "AI analizi kapalı (ayarlardan açılabilir)".to_string(),
                "AI analysis is off (turn it on in Settings)".to_string()
            ),
            AiError::CacheRequired => tr!(
                "AI, yerel önbellek olmadan çalışmaz (cache_enabled = true olmalı)".to_string(),
                "AI does not run without the local cache (cache_enabled must be true)".to_string()
            ),
            AiError::BadProvider(p) => tr!(
                format!("bilinmeyen sağlayıcı: {p}"),
                format!("unknown provider: {p}")
            ),
            AiError::BadBaseUrl(m) => tr!(
                format!("geçersiz base URL: {m}"),
                format!("invalid base URL: {m}")
            ),
            AiError::ModelRequired => tr!(
                "bu sağlayıcı için bir model seçmelisin (\"Modelleri getir\" ile listeyi al)"
                    .to_string(),
                "pick a model for this provider (use \"Fetch models\" to get the list)".to_string()
            ),
            AiError::NoApiKey => tr!(
                "API anahtarı yok (Ayarlar'dan gir ya da DRAXBYTE_API_KEY_<SAĞLAYICI> / DRAXBYTE_API_KEY ortam değişkenini ayarla)".to_string(),
                "no API key (enter it in Settings or set the DRAXBYTE_API_KEY_<PROVIDER> / DRAXBYTE_API_KEY environment variable)".to_string()
            ),
            AiError::LimitReached { used, limit } => tr!(
                format!("günlük token limiti doldu ({used}/{limit}); AI bugün devre dışı"),
                format!("daily token limit reached ({used}/{limit}); AI is off for today")
            ),
            AiError::Cache(m) => tr!(format!("önbellek hatası: {m}"), format!("cache error: {m}")),
            AiError::Http(m) => tr!(
                format!("ağ/HTTP hatası: {m}"),
                format!("network/HTTP error: {m}")
            ),
            AiError::BadResponse(m) => tr!(
                format!("beklenmeyen yanıt: {m}"),
                format!("unexpected response: {m}")
            ),
        };
        f.write_str(&s)
    }
}

impl std::error::Error for AiError {}
