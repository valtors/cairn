//! temporal sqlite engine for agent memory.
//!
//! facts are triples: subject, predicate, object. each carries a validity
//! window (valid_from / valid_until) and a recording time (recorded_at).
//! when a new fact contradicts an old one, the old fact closes. it doesn't
//! delete. you can query the past.
//!
//! one sqlite file. one schema. zero external services.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: String,
    pub valid_until: Option<String>,
    pub recorded_at: String,
    pub confidence: f64,
    pub source: String,
    pub tombstone: bool,
    pub tombstone_reason: Option<String>,
    pub access_count: i64,
    pub last_accessed: Option<String>,
    pub device_id: String,
    pub vector_clock: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberOptions {
    pub valid_from: Option<String>,
    pub recorded_at: Option<String>,
    pub confidence: Option<f64>,
    pub source: Option<String>,
    pub device_id: Option<String>,
}

impl Default for RememberOptions {
    fn default() -> Self {
        Self {
            valid_from: None,
            recorded_at: None,
            confidence: None,
            source: None,
            device_id: None,
        }
    }
}

pub struct Store {
    conn: Connection,
    device_id: String,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P, device_id: Option<String>) -> Result<Self, String> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA synchronous=NORMAL;",
        )
        .map_err(|e| e.to_string())?;
        let store = Self {
            conn,
            device_id: device_id.unwrap_or_else(|| {
                hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "unknown".to_string())
            }),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS facts (
                    id TEXT PRIMARY KEY,
                    subject TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object TEXT NOT NULL,
                    valid_from TEXT NOT NULL,
                    valid_until TEXT,
                    recorded_at TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 1.0,
                    source TEXT NOT NULL DEFAULT 'user',
                    tombstone INTEGER NOT NULL DEFAULT 0,
                    tombstone_reason TEXT,
                    access_count INTEGER NOT NULL DEFAULT 0,
                    last_accessed TEXT,
                    device_id TEXT NOT NULL,
                    vector_clock TEXT NOT NULL DEFAULT '{}'
                );

                CREATE INDEX IF NOT EXISTS idx_facts_subject ON facts(subject);
                CREATE INDEX IF NOT EXISTS idx_facts_predicate ON facts(predicate);
                CREATE INDEX IF NOT EXISTS idx_facts_object ON facts(object);
                CREATE INDEX IF NOT EXISTS idx_facts_active ON facts(tombstone) WHERE tombstone = 0;
                CREATE INDEX IF NOT EXISTS idx_facts_valid ON facts(valid_from, valid_until);
                CREATE INDEX IF NOT EXISTS idx_facts_recorded ON facts(recorded_at);
                CREATE INDEX IF NOT EXISTS idx_facts_sp ON facts(subject, predicate) WHERE tombstone = 0;

                CREATE TABLE IF NOT EXISTS sync_log (
                    peer_id TEXT NOT NULL,
                    last_sync_at TEXT NOT NULL,
                    last_sync_counter INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (peer_id)
                );",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// store a fact. if an active fact with the same (subject, predicate)
    /// exists and the object matches, confidence is bumped and the vector
    /// clock merges. if the object differs, the old fact closes (valid_until
    /// set) and the new one opens. contradiction is not deletion.
    pub fn remember(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        opts: RememberOptions,
    ) -> Result<String, String> {
        let now = Utc::now().to_rfc3339();
        let valid_from = opts.valid_from.unwrap_or_else(|| now.clone());
        let recorded_at = opts.recorded_at.unwrap_or_else(|| now.clone());
        let confidence = opts.confidence.unwrap_or(1.0);
        let source = opts.source.unwrap_or_else(|| "user".to_string());
        let device_id = opts.device_id.unwrap_or_else(|| self.device_id.clone());

        let existing = self.find_active(subject, predicate)?;

        if let Some(ref old) = existing {
            if old.object == object {
                let new_confidence = old.confidence.max(confidence);
                let merged_vc = self.merge_vector_clocks(&old.vector_clock, &device_id);
                self.conn
                    .execute(
                        "UPDATE facts SET confidence = ?, access_count = access_count + 1, vector_clock = ? WHERE id = ?",
                        params![new_confidence, merged_vc, old.id],
                    )
                    .map_err(|e| e.to_string())?;
                return Ok(old.id.clone());
            } else {
                self.conn
                    .execute(
                        "UPDATE facts SET valid_until = ? WHERE id = ?",
                        params![valid_from, old.id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }

        let id = Uuid::new_v4().to_string();
        let vc = serde_json::json!({ device_id.clone(): 1 }).to_string();

        self.conn
            .execute(
                "INSERT INTO facts (id, subject, predicate, object, valid_from, valid_until, recorded_at, confidence, source, device_id, vector_clock)
                 VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
                params![id, subject, predicate, object, valid_from, recorded_at, confidence, source, device_id, vc],
            )
            .map_err(|e| e.to_string())?;

        Ok(id)
    }

    fn find_active(&self, subject: &str, predicate: &str) -> Result<Option<Fact>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE subject = ? AND predicate = ? AND tombstone = 0 AND valid_until IS NULL ORDER BY recorded_at DESC LIMIT 1")
            .map_err(|e| e.to_string())?;
        let fact = stmt
            .query_row(params![subject, predicate], row_to_fact)
            .ok();
        Ok(fact)
    }

    pub fn get_active_facts(&self) -> Result<Vec<Fact>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE tombstone = 0 ORDER BY recorded_at DESC")
            .map_err(|e| e.to_string())?;
        let facts = stmt
            .query_map([], row_to_fact)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(facts)
    }

    pub fn get_active_facts_for(&self, subject: &str) -> Result<Vec<Fact>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE subject = ? AND tombstone = 0 AND valid_until IS NULL ORDER BY recorded_at DESC")
            .map_err(|e| e.to_string())?;
        let facts = stmt
            .query_map(params![subject], row_to_fact)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(facts)
    }

    /// point-in-time query. returns facts that were known before `as_of`
    /// and were still valid at that point. closed facts are included if
    /// they hadn't closed yet.
    pub fn facts_as_of(&self, as_of: &str) -> Result<Vec<Fact>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE tombstone = 0 AND recorded_at <= ? AND (valid_until IS NULL OR valid_until > ?) ORDER BY recorded_at DESC")
            .map_err(|e| e.to_string())?;
        let facts = stmt
            .query_map(params![as_of, as_of], row_to_fact)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(facts)
    }

    pub fn touch(&self, fact_id: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE facts SET access_count = access_count + 1, last_accessed = ? WHERE id = ?",
                params![now, fact_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// soft-delete a fact. the fact stays in the database with a tombstone
    /// flag and a reason. forgetting is auditable.
    pub fn tombstone(&self, fact_id: &str, reason: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE facts SET tombstone = 1, tombstone_reason = ? WHERE id = ?",
                params![reason, fact_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_stale_facts(&self, days: i64, max_confidence: f64) -> Result<Vec<Fact>, String> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let cutoff_str = cutoff.to_rfc3339();
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE tombstone = 0 AND confidence < ? AND (last_accessed IS NULL OR last_accessed < ?)")
            .map_err(|e| e.to_string())?;
        let facts = stmt
            .query_map(params![max_confidence, cutoff_str], row_to_fact)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(facts)
    }

    pub fn all_facts(&self) -> Result<Vec<Fact>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts ORDER BY recorded_at DESC")
            .map_err(|e| e.to_string())?;
        let facts = stmt
            .query_map([], row_to_fact)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        Ok(facts)
    }

    pub fn import_fact(&self, fact: &Fact) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO facts (id, subject, predicate, object, valid_from, valid_until, recorded_at, confidence, source, tombstone, tombstone_reason, access_count, last_accessed, device_id, vector_clock)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    fact.id, fact.subject, fact.predicate, fact.object,
                    fact.valid_from, fact.valid_until, fact.recorded_at,
                    fact.confidence, fact.source, fact.tombstone as i64,
                    fact.tombstone_reason, fact.access_count, fact.last_accessed,
                    fact.device_id, fact.vector_clock
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn merge_vector_clocks(&self, existing: &str, device_id: &str) -> String {
        let mut vc: serde_json::Value =
            serde_json::from_str(existing).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = vc.as_object_mut() {
            let count = obj.get(device_id).and_then(|v| v.as_i64()).unwrap_or(0);
            obj.insert(device_id.to_string(), serde_json::json!(count + 1));
        }
        vc.to_string()
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn row_to_fact(row: &rusqlite::Row) -> rusqlite::Result<Fact> {
    Ok(Fact {
        id: row.get(0)?,
        subject: row.get(1)?,
        predicate: row.get(2)?,
        object: row.get(3)?,
        valid_from: row.get(4)?,
        valid_until: row.get(5)?,
        recorded_at: row.get(6)?,
        confidence: row.get(7)?,
        source: row.get(8)?,
        tombstone: row.get::<_, i64>(9)? != 0,
        tombstone_reason: row.get(10)?,
        access_count: row.get(11)?,
        last_accessed: row.get(12)?,
        device_id: row.get(13)?,
        vector_clock: row.get(14)?,
    })
}
