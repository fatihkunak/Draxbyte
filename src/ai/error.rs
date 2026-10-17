use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI analizi kapalı (ayarlardan açılabilir)")]
    Disabled,
    #[error("AI, yerel önbellek olmadan çalışmaz (cache_enabled = true olmalı)")]
    CacheRequired,
    #[error("bilinmeyen sağlayıcı: {0}")]
    BadProvider(String),
    #[error("geçersiz base URL: {0}")]
    BadBaseUrl(String),
    #[error("bu sağlayıcı için bir model seçmelisin (\"Modelleri getir\" ile listeyi al)")]
    ModelRequired,
    #[error("API anahtarı yok (DRAXBYTE_API_KEY ortam değişkeni ya da ayarlar)")]
    NoApiKey,
    #[error("günlük token limiti doldu ({used}/{limit}); AI bugün devre dışı")]
    LimitReached { used: u64, limit: u64 },
    #[error("önbellek hatası: {0}")]
    Cache(String),
    #[error("ağ/HTTP hatası: {0}")]
    Http(String),
    #[error("beklenmeyen yanıt: {0}")]
    BadResponse(String),
}
