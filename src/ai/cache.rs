//! Local SQLite cache of AI answers plus the daily token counter.
//! A file seen before (same path + size + mtime) never costs a second request.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct AiCache {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub bytes: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> Option<f64> {
        let total = self.hits + self.misses;
        (total > 0).then(|| self.hits as f64 / total as f64)
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS answers (
    key TEXT PRIMARY KEY,
    answer TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS meta (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS usage (day INTEGER PRIMARY KEY, tokens INTEGER NOT NULL);
";

pub fn day_of(unix_secs: i64) -> i64 {
    unix_secs.div_euclid(86_400)
}

impl AiCache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    fn bump(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (name, value) VALUES (?1, 1) ON CONFLICT(name) DO UPDATE SET value = value + 1",
            [name],
        )?;
        Ok(())
    }

    /// Counts a hit or a miss.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let found: Option<String> = self
            .conn
            .query_row("SELECT answer FROM answers WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        self.bump(if found.is_some() { "hits" } else { "misses" })?;
        Ok(found)
    }

    /// Lookup that does not touch the hit/miss counters (used by the UI to decide
    /// whether a request would be free).
    pub fn peek(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT answer FROM answers WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn put(&self, key: &str, answer: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO answers (key, answer, created_at) VALUES (?1, ?2, ?3)",
            params![key, answer, now],
        )?;
        Ok(())
    }

    pub fn add_tokens(&self, day: i64, tokens: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO usage (day, tokens) VALUES (?1, ?2) ON CONFLICT(day) DO UPDATE SET tokens = tokens + ?2",
            params![day, tokens as i64],
        )?;
        Ok(())
    }

    pub fn tokens_on(&self, day: i64) -> Result<u64> {
        let t: Option<i64> = self
            .conn
            .query_row("SELECT tokens FROM usage WHERE day = ?1", [day], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(t.unwrap_or(0).max(0) as u64)
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let meta = |n: &str| -> Result<u64> {
            let v: Option<i64> = self
                .conn
                .query_row("SELECT value FROM meta WHERE name = ?1", [n], |r| r.get(0))
                .optional()?;
            Ok(v.unwrap_or(0).max(0) as u64)
        };
        let (entries, bytes): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(key) + LENGTH(answer)), 0) FROM answers",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(CacheStats {
            entries: entries as u64,
            hits: meta("hits")?,
            misses: meta("misses")?,
            bytes: bytes as u64,
        })
    }

    /// Drops the answers and the hit/miss counters. The token usage is kept on purpose:
    /// clearing the cache must not reset the daily limit.
    pub fn clear(&self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM answers; DELETE FROM meta;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_and_hit_rate() {
        let c = AiCache::open_in_memory().unwrap();
        assert_eq!(c.get("k").unwrap(), None); // miss
        c.put("k", "SAFE: temp file", 10).unwrap();
        assert_eq!(c.get("k").unwrap().as_deref(), Some("SAFE: temp file")); // hit
        assert_eq!(c.get("k").unwrap().as_deref(), Some("SAFE: temp file")); // hit
        let s = c.stats().unwrap();
        assert_eq!((s.entries, s.hits, s.misses), (1, 2, 1));
        assert!((s.hit_rate().unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert!(s.bytes > 0);
    }

    #[test]
    fn put_replaces_existing_answer() {
        let c = AiCache::open_in_memory().unwrap();
        c.put("k", "old", 1).unwrap();
        c.put("k", "new", 2).unwrap();
        assert_eq!(c.get("k").unwrap().as_deref(), Some("new"));
        assert_eq!(c.stats().unwrap().entries, 1);
    }

    #[test]
    fn token_usage_accumulates_per_day_and_survives_clear() {
        let c = AiCache::open_in_memory().unwrap();
        c.add_tokens(100, 120).unwrap();
        c.add_tokens(100, 30).unwrap();
        c.add_tokens(101, 5).unwrap();
        assert_eq!(c.tokens_on(100).unwrap(), 150);
        assert_eq!(c.tokens_on(101).unwrap(), 5);
        assert_eq!(c.tokens_on(99).unwrap(), 0);
        c.put("k", "a", 1).unwrap();
        c.clear().unwrap();
        assert_eq!(c.stats().unwrap(), CacheStats::default());
        assert_eq!(c.tokens_on(100).unwrap(), 150);
    }

    #[test]
    fn empty_cache_has_no_hit_rate() {
        assert!(AiCache::open_in_memory()
            .unwrap()
            .stats()
            .unwrap()
            .hit_rate()
            .is_none());
    }

    #[test]
    fn day_of_is_utc_days() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(86_399), 0);
        assert_eq!(day_of(86_400), 1);
    }
}
