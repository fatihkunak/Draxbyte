//! AI service with every cost guard in front of the network call:
//! enabled flag, mandatory cache, cache lookup, daily token limit, credentials.

use crate::ai::cache::{day_of, AiCache, CacheStats};
pub use crate::ai::error::AiError;
use crate::ai::prompt::{self, Prompt};
use crate::ai::providers;
use crate::config::AiConfig;
use crate::models::{now_secs, ScanItem};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AiAnswer {
    pub text: String,
    pub cached: bool,
    pub tokens: u64,
}

pub struct AiService {
    cfg: AiConfig,
    cache: Mutex<AiCache>,
}

impl AiService {
    /// Fails (and opens nothing, contacts nothing) unless AI is enabled and the cache is on.
    pub fn new(cfg: &AiConfig) -> Result<Self, AiError> {
        Self::check_enabled(cfg)?;
        let path = crate::config::expand_path(&cfg.cache_path);
        let cache = AiCache::open(&path).map_err(|e| AiError::Cache(e.to_string()))?;
        Ok(Self {
            cfg: cfg.clone(),
            cache: Mutex::new(cache),
        })
    }

    pub fn with_cache(cfg: &AiConfig, cache: AiCache) -> Result<Self, AiError> {
        Self::check_enabled(cfg)?;
        Ok(Self {
            cfg: cfg.clone(),
            cache: Mutex::new(cache),
        })
    }

    fn check_enabled(cfg: &AiConfig) -> Result<(), AiError> {
        if !cfg.enabled {
            return Err(AiError::Disabled);
        }
        if !cfg.cache_enabled {
            return Err(AiError::CacheRequired);
        }
        Ok(())
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, AiCache> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn stats(&self) -> Result<CacheStats, AiError> {
        self.cache()
            .stats()
            .map_err(|e| AiError::Cache(e.to_string()))
    }

    pub fn clear_cache(&self) -> Result<(), AiError> {
        self.cache()
            .clear()
            .map_err(|e| AiError::Cache(e.to_string()))
    }

    pub fn tokens_today(&self) -> Result<u64, AiError> {
        self.cache()
            .tokens_on(day_of(now_secs()))
            .map_err(|e| AiError::Cache(e.to_string()))
    }

    pub fn daily_limit(&self) -> u64 {
        self.cfg.daily_token_limit
    }

    /// "Provider · model" for the UI.
    pub fn describe(&self) -> String {
        let label = providers::get(&self.cfg.provider)
            .map(|p| p.label)
            .unwrap_or("?");
        format!("{label} · {}", self.cfg.model)
    }

    /// True when `ask` would be answered from the local cache (free, no request).
    pub fn is_cached(&self, item: &ScanItem) -> bool {
        matches!(self.cache().peek(&item.ai_key()), Ok(Some(_)))
    }

    /// What would be sent, so the UI can show it before the user approves.
    pub fn preview(&self, item: &ScanItem) -> (Prompt, u64) {
        let p = prompt::build(item, now_secs());
        let est = prompt::estimate_tokens(&p);
        (p, est)
    }

    /// Cache first (free); otherwise one guarded request. The caller must have
    /// obtained the user's approval already.
    pub async fn ask(&self, item: &ScanItem) -> Result<AiAnswer, AiError> {
        let key = item.ai_key();
        let cache_err = |e: anyhow::Error| AiError::Cache(e.to_string());

        if let Some(text) = self.cache().get(&key).map_err(cache_err)? {
            return Ok(AiAnswer {
                text,
                cached: true,
                tokens: 0,
            });
        }

        let now = now_secs();
        let (p, est) = self.preview(item);
        let used = self.cache().tokens_on(day_of(now)).map_err(cache_err)?;
        if used + est > self.cfg.daily_token_limit {
            return Err(AiError::LimitReached {
                used,
                limit: self.cfg.daily_token_limit,
            });
        }
        let creds = providers::resolve(&self.cfg)?;
        let (text, tokens) = providers::send_chat(&creds, &p, est).await?;

        let cache = self.cache();
        cache.add_tokens(day_of(now), tokens).map_err(cache_err)?;
        cache.put(&key, &text, now).map_err(cache_err)?;
        Ok(AiAnswer {
            text,
            cached: false,
            tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Category;
    use std::path::PathBuf;

    fn item() -> ScanItem {
        ScanItem {
            path: PathBuf::from("/data/blob.bin"),
            size: 1000,
            mtime: 5,
            is_dir: false,
            category: Category::Yellow,
            rule: "large_file".into(),
            reason: "r".into(),
            unknown: true,
        }
    }

    /// Points at a closed port: any attempt to reach the network fails loudly.
    fn cfg() -> AiConfig {
        AiConfig {
            enabled: true,
            provider: "custom".into(),
            base_url: "http://127.0.0.1:1/v1".into(),
            model: "m".into(),
            api_key: "k-12345678".into(),
            ..AiConfig::default()
        }
    }

    #[test]
    fn disabled_by_default_and_cache_is_mandatory() {
        assert!(matches!(
            AiService::with_cache(&AiConfig::default(), AiCache::open_in_memory().unwrap()),
            Err(AiError::Disabled)
        ));
        let no_cache = AiConfig {
            cache_enabled: false,
            ..cfg()
        };
        assert!(matches!(
            AiService::with_cache(&no_cache, AiCache::open_in_memory().unwrap()),
            Err(AiError::CacheRequired)
        ));
    }

    #[tokio::test]
    async fn cache_hit_makes_no_request_and_costs_nothing() {
        let cache = AiCache::open_in_memory().unwrap();
        let it = item();
        cache.put(&it.ai_key(), "SAFE: scratch file", 1).unwrap();
        let svc = AiService::with_cache(&cfg(), cache).unwrap();
        let a = svc.ask(&it).await.unwrap();
        assert!(a.cached);
        assert_eq!((a.text.as_str(), a.tokens), ("SAFE: scratch file", 0));
        assert_eq!(svc.tokens_today().unwrap(), 0);
    }

    #[tokio::test]
    async fn changed_file_does_not_reuse_old_answer() {
        let cache = AiCache::open_in_memory().unwrap();
        let it = item();
        cache.put(&it.ai_key(), "old", 1).unwrap();
        let svc = AiService::with_cache(&cfg(), cache).unwrap();
        let mut changed = it.clone();
        changed.mtime += 1;
        // miss → tries the network → closed port → Http error (proves it did not serve "old")
        assert!(matches!(svc.ask(&changed).await, Err(AiError::Http(_))));
    }

    #[tokio::test]
    async fn daily_limit_blocks_before_any_request() {
        let cache = AiCache::open_in_memory().unwrap();
        cache.add_tokens(day_of(now_secs()), 49_950).unwrap();
        let svc = AiService::with_cache(&cfg(), cache).unwrap();
        assert!(matches!(
            svc.ask(&item()).await,
            Err(AiError::LimitReached { .. })
        ));
    }

    #[tokio::test]
    async fn missing_api_key_is_reported_before_network() {
        std::env::remove_var(crate::config::API_KEY_ENV);
        let c = AiConfig {
            provider: "openai".into(),
            api_key: String::new(),
            ..cfg()
        };
        let svc = AiService::with_cache(&c, AiCache::open_in_memory().unwrap()).unwrap();
        assert!(matches!(svc.ask(&item()).await, Err(AiError::NoApiKey)));
    }

    #[tokio::test]
    async fn successful_request_is_cached_and_counted() {
        // tiny local server answering one chat request
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf).unwrap();
            let body = r#"{"choices":[{"message":{"content":"SAFE: scratch"}}],"usage":{"total_tokens":55}}"#;
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes());
        });
        let c = AiConfig {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            ..cfg()
        };
        let svc = AiService::with_cache(&c, AiCache::open_in_memory().unwrap()).unwrap();
        let first = svc.ask(&item()).await.unwrap();
        assert_eq!(
            (first.cached, first.tokens, first.text.as_str()),
            (false, 55, "SAFE: scratch")
        );
        assert_eq!(svc.tokens_today().unwrap(), 55);
        // second time: from cache, server is gone → would fail if it tried the network
        let second = svc.ask(&item()).await.unwrap();
        assert!(second.cached);
        assert_eq!(svc.tokens_today().unwrap(), 55);
    }
}
