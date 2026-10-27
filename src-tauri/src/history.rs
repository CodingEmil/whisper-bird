//! Verlauf und Statistiken (SQLite). Statistiken werden separat gezählt,
//! damit Löschen im Verlauf sie nicht zurücksetzt.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub id: i64,
    pub created_at: i64,
    pub text: String,
    pub raw_text: String,
    pub source: String,
    pub app: String,
    pub language: Option<String>,
    pub duration_ms: i64,
    pub words: i64,
    pub model: String,
}

pub struct NewEntry {
    pub text: String,
    pub raw_text: String,
    /// "dictation" | "enhanced" | "edit" | "file" | "meeting"
    pub source: &'static str,
    pub app: String,
    pub language: Option<String>,
    pub duration_ms: i64,
    pub model: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    pub search: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub oldest_first: bool,
    pub source: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LanguageStat {
    pub language: String,
    pub count: i64,
    pub words: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DayStat {
    pub day: String,
    pub count: i64,
    pub words: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Stats {
    pub count: i64,
    pub duration_ms: i64,
    pub words: i64,
    pub enhanced: i64,
    pub edits: i64,
    pub files: i64,
    pub meetings: i64,
    pub languages: Vec<LanguageStat>,
    pub days: Vec<DayStat>,
    pub first_use: Option<i64>,
}

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS entries (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               created_at INTEGER NOT NULL,
               text TEXT NOT NULL,
               raw_text TEXT NOT NULL,
               source TEXT NOT NULL,
               app TEXT NOT NULL DEFAULT '',
               language TEXT,
               duration_ms INTEGER NOT NULL DEFAULT 0,
               words INTEGER NOT NULL DEFAULT 0,
               model TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS idx_entries_created ON entries(created_at);
             CREATE TABLE IF NOT EXISTS stats (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS lang_stats (language TEXT PRIMARY KEY, count INTEGER NOT NULL, words INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS daily_stats (day TEXT PRIMARY KEY, count INTEGER NOT NULL, words INTEGER NOT NULL);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn add(&self, e: NewEntry, limit: u32) -> anyhow::Result<Entry> {
        let now = chrono::Utc::now().timestamp_millis();
        let words = crate::text::word_count(&e.text) as i64;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO entries (created_at, text, raw_text, source, app, language, duration_ms, words, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![now, e.text, e.raw_text, e.source, e.app, e.language, e.duration_ms, words, e.model],
        )?;
        let id = conn.last_insert_rowid();

        let bump = |key: &str, by: i64| {
            conn.execute(
                "INSERT INTO stats (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = value + excluded.value",
                params![key, by],
            )
        };
        bump("count", 1)?;
        bump("duration_ms", e.duration_ms)?;
        bump("words", words)?;
        match e.source {
            "enhanced" => bump("enhanced", 1)?,
            "edit" => bump("edits", 1)?,
            "file" => bump("files", 1)?,
            "meeting" => bump("meetings", 1)?,
            _ => 0,
        };
        conn.execute(
            "INSERT INTO stats (key, value) VALUES ('first_use', ?1) ON CONFLICT(key) DO NOTHING",
            params![now],
        )?;
        if let Some(lang) = &e.language {
            conn.execute(
                "INSERT INTO lang_stats (language, count, words) VALUES (?1, 1, ?2)
                 ON CONFLICT(language) DO UPDATE SET count = count + 1, words = words + excluded.words",
                params![lang, words],
            )?;
        }
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO daily_stats (day, count, words) VALUES (?1, 1, ?2)
             ON CONFLICT(day) DO UPDATE SET count = count + 1, words = words + excluded.words",
            params![day, words],
        )?;
        if limit > 0 {
            conn.execute(
                "DELETE FROM entries WHERE id NOT IN (SELECT id FROM entries ORDER BY created_at DESC LIMIT ?1)",
                params![limit],
            )?;
        }
        Ok(Entry {
            id,
            created_at: now,
            text: e.text,
            raw_text: e.raw_text,
            source: e.source.to_string(),
            app: e.app,
            language: e.language,
            duration_ms: e.duration_ms,
            words,
            model: e.model,
        })
    }

    pub fn query(&self, q: &Query) -> anyhow::Result<Vec<Entry>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, created_at, text, raw_text, source, app, language, duration_ms, words, model FROM entries WHERE 1=1",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if !q.search.trim().is_empty() {
            sql.push_str(" AND (text LIKE ?1 OR raw_text LIKE ?1 OR app LIKE ?1)");
            args.push(Box::new(format!("%{}%", q.search.trim())));
        }
        if let Some(from) = q.from {
            sql.push_str(&format!(" AND created_at >= ?{}", args.len() + 1));
            args.push(Box::new(from));
        }
        if let Some(to) = q.to {
            sql.push_str(&format!(" AND created_at <= ?{}", args.len() + 1));
            args.push(Box::new(to));
        }
        if let Some(source) = &q.source {
            sql.push_str(&format!(" AND source = ?{}", args.len() + 1));
            args.push(Box::new(source.clone()));
        }
        sql.push_str(if q.oldest_first { " ORDER BY created_at ASC" } else { " ORDER BY created_at DESC" });
        sql.push_str(&format!(
            " LIMIT {} OFFSET {}",
            q.limit.unwrap_or(200).min(2000),
            q.offset.unwrap_or(0)
        ));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), row_to_entry)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn last(&self) -> Option<Entry> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, created_at, text, raw_text, source, app, language, duration_ms, words, model
             FROM entries ORDER BY created_at DESC LIMIT 1",
            [],
            row_to_entry,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM entries", [])?;
        Ok(())
    }

    pub fn reset_stats(&self) -> anyhow::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute_batch("DELETE FROM stats; DELETE FROM lang_stats; DELETE FROM daily_stats;")?;
        Ok(())
    }

    pub fn stats(&self) -> anyhow::Result<Stats> {
        let conn = self.conn.lock().unwrap();
        let get = |key: &str| -> i64 {
            conn.query_row("SELECT value FROM stats WHERE key = ?1", params![key], |r| r.get(0))
                .unwrap_or(0)
        };
        let mut stats = Stats {
            count: get("count"),
            duration_ms: get("duration_ms"),
            words: get("words"),
            enhanced: get("enhanced"),
            edits: get("edits"),
            files: get("files"),
            meetings: get("meetings"),
            first_use: Some(get("first_use")).filter(|v| *v > 0),
            ..Default::default()
        };
        let mut stmt = conn.prepare("SELECT language, count, words FROM lang_stats ORDER BY count DESC")?;
        stats.languages = stmt
            .query_map([], |r| {
                Ok(LanguageStat {
                    language: r.get(0)?,
                    count: r.get(1)?,
                    words: r.get(2)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        let mut stmt = conn.prepare("SELECT day, count, words FROM daily_stats ORDER BY day DESC LIMIT 30")?;
        stats.days = stmt
            .query_map([], |r| {
                Ok(DayStat {
                    day: r.get(0)?,
                    count: r.get(1)?,
                    words: r.get(2)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        stats.days.reverse();
        Ok(stats)
    }
}

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        created_at: r.get(1)?,
        text: r.get(2)?,
        raw_text: r.get(3)?,
        source: r.get(4)?,
        app: r.get(5)?,
        language: r.get(6)?,
        duration_ms: r.get(7)?,
        words: r.get(8)?,
        model: r.get(9)?,
    })
}
