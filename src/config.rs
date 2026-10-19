//! `config.toml` loading, saving and defaults.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const API_KEY_ENV: &str = "DRAXBYTE_API_KEY";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Bumped when a default changes in a way old config files must pick up (see `migrate`).
    pub config_version: u32,
    pub scan: ScanConfig,
    pub rules: RulesConfig,
    pub ai: AiConfig,
    pub cleanup: CleanupConfig,
    pub ui: UiConfig,
    pub memory: MemoryConfig,
}

/// Memory screen settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MemoryConfig {
    /// Program names (e.g. `my-service.exe`) that can never be closed from Draxbyte.
    pub protected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub default_paths: Vec<String>,
    pub max_depth: usize,
    pub min_size_mb: u64,
    pub exclude_patterns: Vec<String>,
    /// Also scan system temp/log/cache folders (`C:\Windows\Temp`, `Package Cache`, …).
    /// Only applies when the scan paths are the defaults.
    pub include_system: bool,
    /// Also scan every internal drive, not just the home folder.
    pub all_drives: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    pub safe_extensions: Vec<String>,
    pub safe_age_days: i64,
    pub large_file_mb: u64,
}

/// One link of the AI advisor's failover chain (the first enabled one is the primary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainProvider {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Key of this entry only. Prefer the `DRAXBYTE_API_KEY_<PROVIDER>` environment variable.
    pub api_key: String,
}

impl Default for ChainProvider {
    fn default() -> Self {
        Self { enabled: true, provider: String::new(), base_url: String::new(), model: String::new(), api_key: String::new() }
    }
}

/// A provider the advisor will try, with its key already worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorTarget {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub key: Option<String>,
}

/// `DRAXBYTE_API_KEY_OPENAI`, `DRAXBYTE_API_KEY_NVIDIA_NIM`…
pub fn provider_key_env(provider: &str) -> String {
    let id: String = provider.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("{API_KEY_ENV}_{id}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Legacy single key (still honoured when no per-provider key exists).
    pub api_key: String,
    /// Keys per provider id. Prefer the `DRAXBYTE_API_KEY` environment variable.
    pub api_keys: std::collections::BTreeMap<String, String>,
    pub daily_token_limit: u64,
    pub cache_enabled: bool,
    pub cache_path: String,
    /// Failover chain of the AI advisor. Empty = just the single provider above.
    pub chain: Vec<ChainProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// `auto` (follow the system), `tr` or `en`.
    pub language: String,
    /// Ask GitHub for a newer release at start-up. Off by default: no network request otherwise.
    pub check_updates: bool,
    /// Windows: ask for administrator rights when the app starts (one UAC prompt), so no folder is
    /// out of reach for scans and removals. On by default; declining the prompt just starts the
    /// app without them.
    pub always_admin: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            check_updates: false,
            always_admin: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    pub use_trash: bool,
    pub confirm_permanent_delete: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        let mut default_paths = vec!["~".to_string()];
        default_paths.push(std::env::temp_dir().to_string_lossy().into_owned());
        if cfg!(unix) {
            default_paths.push("/tmp".into());
            default_paths.push("/var/tmp".into());
        }
        default_paths.dedup();
        Self {
            default_paths,
            max_depth: 64,
            min_size_mb: 1,
            exclude_patterns: vec!["*.git/*".into(), "node_modules/*".into()],
            include_system: true,
            all_drives: true,
        }
    }
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            safe_extensions: ["tmp", "log", "cache", "dmp", "old", "bak"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            safe_age_days: 30,
            large_file_mb: 500,
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai".into(),
            base_url: String::new(), // only used by custom / local / workspace providers
            model: "gpt-4o-mini".into(),
            api_key: String::new(),
            api_keys: Default::default(),
            daily_token_limit: 50_000,
            cache_enabled: true,
            cache_path: "~/.draxbyte/cache.db".into(),
            chain: Vec::new(),
        }
    }
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            use_trash: true,
            confirm_permanent_delete: true,
        }
    }
}

impl AiConfig {
    /// Environment variable wins (the provider's own `DRAXBYTE_API_KEY_<PROVIDER>`, then the general
    /// `DRAXBYTE_API_KEY`), then the key stored for the selected provider, then the legacy single key.
    pub fn resolved_api_key(&self) -> Option<String> {
        let nonempty = |k: &str| Some(k.trim().to_string()).filter(|k| !k.is_empty());
        std::env::var(provider_key_env(&self.provider))
            .ok()
            .and_then(|k| nonempty(&k))
            .or_else(|| std::env::var(API_KEY_ENV).ok().and_then(|k| nonempty(&k)))
            .or_else(|| self.api_keys.get(&self.provider).and_then(|k| nonempty(k)))
            .or_else(|| nonempty(&self.api_key))
    }
}

impl AiConfig {
    /// Key of a chain entry: its own environment variable, then its own key, then the key stored
    /// for that provider. Never the global `DRAXBYTE_API_KEY`: with several providers in a chain
    /// that one variable would send a single provider's key to all of them.
    pub fn chain_key(&self, e: &ChainProvider) -> Option<String> {
        let nonempty = |k: &str| Some(k.trim().to_string()).filter(|k| !k.is_empty());
        std::env::var(provider_key_env(&e.provider))
            .ok()
            .and_then(|k| nonempty(&k))
            .or_else(|| nonempty(&e.api_key))
            .or_else(|| self.api_keys.get(&e.provider).and_then(|k| nonempty(k)))
    }

    /// What the advisor tries, primary first: the enabled chain entries, or, when no chain is set,
    /// the single provider of `[ai]` (which may use the global key variable: it is the only one).
    pub fn advisor_targets(&self) -> Vec<AdvisorTarget> {
        let entries: Vec<&ChainProvider> = self.chain.iter().filter(|e| e.enabled && !e.provider.is_empty()).collect();
        if entries.is_empty() {
            if self.provider.is_empty() {
                return Vec::new();
            }
            return vec![AdvisorTarget { provider: self.provider.clone(), base_url: self.base_url.clone(), model: self.model.clone(), key: self.resolved_api_key() }];
        }
        entries
            .into_iter()
            .map(|e| AdvisorTarget { provider: e.provider.clone(), base_url: e.base_url.clone(), model: e.model.clone(), key: self.chain_key(e) })
            .collect()
    }
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn expand_path(p: &str) -> PathBuf {
    if p == "~" {
        home_dir()
    } else if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        home_dir().join(rest)
    } else {
        PathBuf::from(p)
    }
}

/// Where config, scan database and AI cache live. `DRAXBYTE_HOME` overrides
/// `~/.draxbyte` (portable installs, screenshots, tests).
pub fn data_dir() -> PathBuf {
    match std::env::var_os("DRAXBYTE_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home_dir().join(".draxbyte"),
    }
}

pub fn default_config_path() -> PathBuf {
    data_dir().join("config.toml")
}

/// Current `config_version`.
pub const CONFIG_VERSION: u32 = 2;

impl Config {
    /// v2: the scan depth default went 10 → 64 (deep trees were silently cut off). A saved 10 is
    /// the old default, not a choice, so it moves too.
    fn migrate(&mut self) {
        if self.config_version < 2 && self.scan.max_depth == 10 {
            self.scan.max_depth = 64;
        }
        self.config_version = CONFIG_VERSION;
    }

    /// Missing file → defaults (nothing is written until the user saves).
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            let mut c = Self::default();
            c.migrate();
            return Ok(c);
        }
        let text = std::fs::read_to_string(path).with_context(|| {
            crate::tf!(
                "config okunamadı: {}",
                "could not read config: {}",
                path.display()
            )
        })?;
        let mut cfg: Self = toml::from_str(&text).with_context(|| {
            crate::tf!("config geçersiz: {}", "invalid config: {}", path.display())
        })?;
        cfg.migrate();
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provider_chain_survives_save_and_load_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.toml");
        let mut c = Config::default();
        c.ai.enabled = true;
        for (p, m) in [("groq", "llama"), ("openai", "gpt-4o-mini"), ("ollama", "qwen")] {
            c.ai.chain.push(ChainProvider { provider: p.into(), model: m.into(), api_key: format!("key-{p}"), ..Default::default() });
        }
        c.ai.chain[1].enabled = false;
        c.save(&f).unwrap();
        let back = Config::load(&f).unwrap();
        assert_eq!(back.ai.chain, c.ai.chain);
        // the disabled middle entry is skipped, the order of the others is kept
        let t = back.ai.advisor_targets();
        assert_eq!(t.iter().map(|x| x.provider.as_str()).collect::<Vec<_>>(), vec!["groq", "ollama"]);
    }

    #[test]
    fn the_single_provider_reads_its_own_environment_variable_first() {
        let mut c = AiConfig { provider: "zz-single-test".into(), ..AiConfig::default() };
        c.api_keys.insert("zz-single-test".into(), "stored".into());
        assert_eq!(c.resolved_api_key().as_deref(), Some("stored"));
        std::env::set_var("DRAXBYTE_API_KEY_ZZ_SINGLE_TEST", "from-own-env");
        assert_eq!(c.resolved_api_key().as_deref(), Some("from-own-env"), "DRAXBYTE_API_KEY_<PROVIDER> works without a chain too");
        std::env::remove_var("DRAXBYTE_API_KEY_ZZ_SINGLE_TEST");
    }

    #[test]
    fn every_chain_entry_gets_only_its_own_key() {
        let mut c = AiConfig::default();
        c.api_keys.insert("openai".into(), "stored-openai".into());
        let own = ChainProvider { provider: "groq".into(), api_key: "own-groq".into(), ..Default::default() };
        let stored = ChainProvider { provider: "openai".into(), ..Default::default() };
        let none = ChainProvider { provider: "mistral".into(), ..Default::default() };
        assert_eq!(c.chain_key(&own).as_deref(), Some("own-groq"));
        assert_eq!(c.chain_key(&stored).as_deref(), Some("stored-openai"));
        assert_eq!(c.chain_key(&none), None, "a provider without a key never borrows another's");
        assert_eq!(provider_key_env("nvidia-nim"), "DRAXBYTE_API_KEY_NVIDIA_NIM");
        // per-provider variable (a name nothing else uses, so tests cannot race)
        let z = ChainProvider { provider: "zz-draxbyte-test".into(), ..Default::default() };
        std::env::set_var("DRAXBYTE_API_KEY_ZZ_DRAXBYTE_TEST", "from-env");
        assert_eq!(c.chain_key(&z).as_deref(), Some("from-env"));
        std::env::remove_var("DRAXBYTE_API_KEY_ZZ_DRAXBYTE_TEST");
    }

    #[test]
    fn without_a_chain_the_single_provider_is_the_only_target() {
        let c = AiConfig { provider: "groq".into(), model: "m".into(), ..AiConfig::default() };
        let t = c.advisor_targets();
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].provider.as_str(), t[0].model.as_str()), ("groq", "m"));
    }

    #[test]
    fn old_config_with_depth_10_is_migrated_once() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.toml");
        std::fs::write(&f, "[scan]
max_depth = 10
").unwrap();
        let c = Config::load(&f).unwrap();
        assert_eq!(c.scan.max_depth, 64);
        assert_eq!(c.config_version, CONFIG_VERSION);
        // a deliberate value on a current config is left alone
        std::fs::write(&f, "config_version = 2
[scan]
max_depth = 10
").unwrap();
        assert_eq!(Config::load(&f).unwrap().scan.max_depth, 10);
        // a deliberate non-default depth on an old config is left alone too
        std::fs::write(&f, "[scan]
max_depth = 5
").unwrap();
        assert_eq!(Config::load(&f).unwrap().scan.max_depth, 5);
    }

    #[test]
    fn missing_file_gives_defaults_with_ai_off() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(&dir.path().join("nope.toml")).unwrap();
        assert!(!cfg.ai.enabled);
        assert!(cfg.ai.cache_enabled);
        assert_eq!(cfg.ai.daily_token_limit, 50_000);
        assert!(cfg.cleanup.use_trash);
    }

    #[test]
    fn partial_file_fills_defaults_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.toml");
        std::fs::write(&p, "[rules]\nsafe_age_days = 7\n").unwrap();
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.rules.safe_age_days, 7);
        assert_eq!(cfg.rules.large_file_mb, 500);
        cfg.save(&p).unwrap();
        assert_eq!(Config::load(&p).unwrap().rules.safe_age_days, 7);
    }

    #[test]
    fn per_provider_keys_roundtrip_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.toml");
        let mut cfg = Config::default();
        cfg.ai.provider = "groq".into();
        cfg.ai.model = "some-model".into();
        cfg.ai.api_keys.insert("groq".into(), "gsk_abcdefgh".into());
        cfg.ai
            .api_keys
            .insert("openai".into(), "sk-12345678".into());
        cfg.save(&p).unwrap();
        let back = Config::load(&p).unwrap();
        assert_eq!(back.ai.provider, "groq");
        assert_eq!(
            back.ai.api_keys.get("openai").map(String::as_str),
            Some("sk-12345678")
        );
        std::env::remove_var(API_KEY_ENV);
        assert_eq!(back.ai.resolved_api_key().as_deref(), Some("gsk_abcdefgh"));
    }

    #[test]
    fn draxbyte_home_overrides_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("DRAXBYTE_HOME", dir.path());
        assert_eq!(data_dir(), dir.path());
        assert_eq!(default_config_path(), dir.path().join("config.toml"));
        std::env::remove_var("DRAXBYTE_HOME");
        assert!(data_dir().ends_with(".draxbyte"));
    }

    #[test]
    fn tilde_expands() {
        assert_eq!(expand_path("~"), home_dir());
        assert_eq!(expand_path("~/x"), home_dir().join("x"));
        assert_eq!(expand_path("/abs"), PathBuf::from("/abs"));
    }
}
