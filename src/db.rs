//! SQLite: scan sessions and the cleanup log.

use crate::models::{now_secs, Category, CleanReport, Outcome, ScanItem, ScanSummary};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

pub struct Db {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS scan_session (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    roots TEXT NOT NULL,
    item_count INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    green_bytes INTEGER NOT NULL,
    files_visited INTEGER NOT NULL,
    errors INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS scan_item (
    session_id INTEGER NOT NULL REFERENCES scan_session(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    is_dir INTEGER NOT NULL,
    category TEXT NOT NULL,
    rule TEXT NOT NULL,
    reason TEXT NOT NULL,
    unknown INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scan_item_session ON scan_item(session_id);
CREATE TABLE IF NOT EXISTS clean_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    action TEXT NOT NULL,
    detail TEXT NOT NULL
);
";

impl Db {
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
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Stores a finished scan and returns its session id. Older sessions are pruned
    /// (the last 5 are kept) so the database does not grow without bound.
    pub fn save_session(&mut self, summary: &ScanSummary, items: &[ScanItem]) -> Result<i64> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scan_session (started_at, finished_at, roots, item_count, total_bytes, green_bytes, files_visited, errors)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                summary.started_at,
                summary.finished_at,
                summary.roots.join("\n"),
                summary.item_count as i64,
                summary.total_bytes as i64,
                summary.green_bytes as i64,
                summary.files_visited as i64,
                summary.errors as i64
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO scan_item (session_id, path, size, mtime, is_dir, category, rule, reason, unknown)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )?;
            for i in items {
                stmt.execute(params![
                    id,
                    i.path.to_string_lossy(),
                    i.size as i64,
                    i.mtime,
                    i.is_dir,
                    i.category.as_str(),
                    i.rule,
                    i.reason,
                    i.unknown
                ])?;
            }
        }
        tx.execute(
            "DELETE FROM scan_session WHERE id NOT IN (SELECT id FROM scan_session ORDER BY id DESC LIMIT 5)",
            [],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn last_summary(&self) -> Result<Option<ScanSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, finished_at, roots, item_count, total_bytes, green_bytes, files_visited, errors
             FROM scan_session ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |r| {
            let roots: String = r.get(3)?;
            Ok(ScanSummary {
                session_id: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                roots: roots.lines().map(String::from).collect(),
                item_count: r.get::<_, i64>(4)? as u64,
                total_bytes: r.get::<_, i64>(5)? as u64,
                green_bytes: r.get::<_, i64>(6)? as u64,
                files_visited: r.get::<_, i64>(7)? as u64,
                errors: r.get::<_, i64>(8)? as u64,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn load_items(&self, session_id: i64) -> Result<Vec<ScanItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, size, mtime, is_dir, category, rule, reason, unknown
             FROM scan_item WHERE session_id = ?1 ORDER BY size DESC",
        )?;
        let rows = stmt.query_map([session_id], |r| {
            let cat: String = r.get(4)?;
            Ok(ScanItem {
                path: PathBuf::from(r.get::<_, String>(0)?),
                size: r.get::<_, i64>(1)? as u64,
                mtime: r.get(2)?,
                is_dir: r.get(3)?,
                category: Category::parse(&cat).unwrap_or(Category::Red),
                rule: r.get(5)?,
                reason: r.get(6)?,
                unknown: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn log_clean(&mut self, report: &CleanReport) -> Result<()> {
        let ts = now_secs();
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO clean_log (ts, path, size, action, detail) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for e in &report.entries {
                let (action, detail) = match &e.outcome {
                    Outcome::DryRun => ("dry_run", String::new()),
                    Outcome::Trashed => ("trashed", String::new()),
                    Outcome::Deleted => ("deleted", String::new()),
                    Outcome::Skipped(m) => ("skipped", m.clone()),
                    Outcome::Failed(m) => ("failed", m.clone()),
                };
                stmt.execute(params![
                    ts,
                    e.path.to_string_lossy(),
                    e.size as i64,
                    action,
                    detail
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clean_log_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM clean_log", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CleanEntry;

    fn item(name: &str, size: u64) -> ScanItem {
        ScanItem {
            path: PathBuf::from(name),
            size,
            mtime: 5,
            is_dir: false,
            category: Category::Green,
            rule: "safe_ext".into(),
            reason: "r".into(),
            unknown: false,
        }
    }

    #[test]
    fn session_roundtrip() {
        let mut db = Db::open_in_memory().unwrap();
        assert!(db.last_summary().unwrap().is_none());
        let items = vec![item("/a", 30), item("/b", 10)];
        let summary = ScanSummary {
            roots: vec!["/x".into(), "/y".into()],
            item_count: 2,
            total_bytes: 40,
            ..Default::default()
        };
        let id = db.save_session(&summary, &items).unwrap();
        let s = db.last_summary().unwrap().unwrap();
        assert_eq!((s.session_id, s.item_count, s.roots.len()), (id, 2, 2));
        let loaded = db.load_items(id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].size, 30); // size-desc
        assert_eq!(loaded[0].category, Category::Green);
    }

    #[test]
    fn old_sessions_are_pruned() {
        let mut db = Db::open_in_memory().unwrap();
        for _ in 0..8 {
            db.save_session(&ScanSummary::default(), &[item("/a", 1)])
                .unwrap();
        }
        let n: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM scan_session", [], |r| r.get(0))
            .unwrap();
        let items: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM scan_item", [], |r| r.get(0))
            .unwrap();
        assert_eq!((n, items), (5, 5)); // cascade removed the pruned sessions' items
    }

    #[test]
    fn clean_log_records_every_entry() {
        let mut db = Db::open_in_memory().unwrap();
        let report = CleanReport {
            entries: vec![
                CleanEntry {
                    path: "/a".into(),
                    size: 1,
                    outcome: Outcome::Trashed,
                },
                CleanEntry {
                    path: "/b".into(),
                    size: 2,
                    outcome: Outcome::Skipped("kilitli".into()),
                },
            ],
            ..Default::default()
        };
        db.log_clean(&report).unwrap();
        assert_eq!(db.clean_log_count().unwrap(), 2);
    }
}
