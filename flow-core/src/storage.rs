//! SQLite storage layer for persisting transcriptions, shortcuts, corrections, and events

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use tracing::{debug, info};
use uuid::Uuid;

use crate::error::Result;
use crate::types::{
    AnalyticsEvent, AppCategory, AppContext, Contact, ContactCategory, Correction,
    CorrectionSource, EventType, Shortcut, Transcription, TranscriptionHistoryEntry,
    TranscriptionStatus, WritingMode,
};

/// Storage backend using SQLite
pub struct Storage {
    conn: Mutex<Connection>,
}

pub const SETTING_OPENAI_API_KEY: &str = "openai_api_key";
pub const SETTING_GEMINI_API_KEY: &str = "gemini_api_key";
pub const SETTING_ANTHROPIC_API_KEY: &str = "anthropic_api_key";
pub const SETTING_OPENROUTER_API_KEY: &str = "openrouter_api_key";
pub const SETTING_BASE10_API_KEY: &str = "base10_api_key";
pub const SETTING_COMPLETION_PROVIDER: &str = "completion_provider";
pub const SETTING_USE_LOCAL_TRANSCRIPTION: &str = "use_local_transcription";
pub const SETTING_LOCAL_WHISPER_MODEL: &str = "local_whisper_model";
/// Cloud transcription provider: "auto" (default) | "openai"
pub const SETTING_CLOUD_TRANSCRIPTION_PROVIDER: &str = "cloud_transcription_provider";

impl Storage {
    /// Open or create a database at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Create an in-memory database (useful for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS transcriptions (
                id TEXT PRIMARY KEY,
                raw_text TEXT NOT NULL,
                processed_text TEXT NOT NULL,
                confidence REAL NOT NULL,
                duration_ms INTEGER NOT NULL,
                app_name TEXT,
                bundle_id TEXT,
                window_title TEXT,
                app_category TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transcription_history (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                text TEXT NOT NULL,
                error TEXT,
                duration_ms INTEGER NOT NULL,
                app_name TEXT,
                bundle_id TEXT,
                window_title TEXT,
                app_category TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS shortcuts (
                id TEXT PRIMARY KEY,
                trigger TEXT NOT NULL UNIQUE,
                replacement TEXT NOT NULL,
                case_sensitive INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                use_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS corrections (
                id TEXT PRIMARY KEY,
                original TEXT NOT NULL,
                corrected TEXT NOT NULL,
                occurrences INTEGER NOT NULL DEFAULT 1,
                confidence REAL NOT NULL DEFAULT 0.5,
                source TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(original, corrected)
            );

            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                properties TEXT NOT NULL,
                app_name TEXT,
                bundle_id TEXT,
                window_title TEXT,
                app_category TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_modes (
                app_name TEXT PRIMARY KEY,
                writing_mode TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS style_samples (
                id TEXT PRIMARY KEY,
                app_name TEXT NOT NULL,
                sample_text TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS contacts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                organization TEXT,
                category TEXT NOT NULL,
                frequency INTEGER NOT NULL DEFAULT 0,
                last_contacted TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_transcriptions_created ON transcriptions(created_at);
            CREATE INDEX IF NOT EXISTS idx_shortcuts_trigger ON shortcuts(trigger);
            CREATE INDEX IF NOT EXISTS idx_corrections_original ON corrections(original);
            CREATE INDEX IF NOT EXISTS idx_transcription_history_created ON transcription_history(created_at);
            CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
            CREATE INDEX IF NOT EXISTS idx_style_samples_app ON style_samples(app_name);
            CREATE INDEX IF NOT EXISTS idx_contacts_name ON contacts(name);
            CREATE INDEX IF NOT EXISTS idx_contacts_frequency ON contacts(frequency DESC);
            "#,
        )?;

        // Migration: Add raw_text column to transcription_history if it doesn't exist
        let _ = conn.execute(
            "ALTER TABLE transcription_history ADD COLUMN raw_text TEXT NOT NULL DEFAULT ''",
            [],
        );

        // Seed default corrections (only if table is empty)
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM corrections",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            let now = Utc::now().to_rfc3339();

            // Seed common transcription corrections
            let seeds = [
                ("u of t hacks", "UofTHacks"),
                ("get hub", "GitHub"),
                ("anthropic", "Anthropic"),
                ("open ai", "OpenAI"),
                ("chat gpt", "ChatGPT"),
                ("gonna", "going to"),
                ("wanna", "want to"),
                ("kinda", "kind of"),
            ];

            for (original, corrected) in seeds {
                conn.execute(
                    r#"
                    INSERT OR IGNORE INTO corrections (id, original, corrected, occurrences, confidence, source, created_at, updated_at)
                    VALUES (?1, ?2, ?3, 3, 0.75, 'Seeded', ?4, ?4)
                    "#,
                    params![Uuid::new_v4().to_string(), original, corrected, now],
                )?;
            }
            debug!("Seeded {} default corrections", seeds.len());
        }

        info!("Database schema initialized");
        Ok(())
    }

    // ========== Transcription methods ==========

    /// Save a transcription
    pub fn save_transcription(&self, transcription: &Transcription) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO transcriptions (id, raw_text, processed_text, confidence, duration_ms,
                                        app_name, bundle_id, window_title, app_category, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                transcription.id.to_string(),
                transcription.raw_text,
                transcription.processed_text,
                transcription.confidence,
                transcription.duration_ms as i64,
                transcription.app_context.as_ref().map(|c| &c.app_name),
                transcription
                    .app_context
                    .as_ref()
                    .and_then(|c| c.bundle_id.as_ref()),
                transcription
                    .app_context
                    .as_ref()
                    .and_then(|c| c.window_title.as_ref()),
                transcription
                    .app_context
                    .as_ref()
                    .map(|c| format!("{:?}", c.category)),
                transcription.created_at.to_rfc3339(),
            ],
        )?;
        debug!("Saved transcription {}", transcription.id);
        Ok(())
    }

    // ========== Settings ==========

    /// Save or update a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Get a setting value
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// Get recent transcriptions
    pub fn get_recent_transcriptions(&self, limit: usize) -> Result<Vec<Transcription>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, raw_text, processed_text, confidence, duration_ms,
                   app_name, bundle_id, window_title, app_category, created_at
            FROM transcriptions
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;

        let transcriptions = stmt
            .query_map([limit as i64], |row| {
                let id: String = row.get(0)?;
                let app_name: Option<String> = row.get(5)?;
                let bundle_id: Option<String> = row.get(6)?;
                let window_title: Option<String> = row.get(7)?;
                let app_category_str: Option<String> = row.get(8)?;
                let created_at_str: String = row.get(9)?;

                let app_context = app_name.map(|name| {
                    let category = app_category_str
                        .as_ref()
                        .and_then(|s| parse_app_category(s))
                        .unwrap_or(AppCategory::Unknown);
                    AppContext {
                        app_name: name,
                        bundle_id,
                        window_title,
                        category,
                    }
                });

                Ok(Transcription {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    raw_text: row.get(1)?,
                    processed_text: row.get(2)?,
                    confidence: row.get(3)?,
                    duration_ms: row.get::<_, i64>(4)? as u64,
                    app_context,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(transcriptions)
    }

    /// Save a transcription history entry
    pub fn save_history_entry(&self, entry: &TranscriptionHistoryEntry) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO transcription_history (id, status, text, raw_text, error, duration_ms,
                                               app_name, bundle_id, window_title, app_category, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                entry.id.to_string(),
                match entry.status {
                    TranscriptionStatus::Success => "success",
                    TranscriptionStatus::Failed => "failed",
                },
                entry.text,
                entry.raw_text,
                entry.error,
                entry.duration_ms as i64,
                entry.app_context.as_ref().map(|c| &c.app_name),
                entry.app_context.as_ref().and_then(|c| c.bundle_id.as_ref()),
                entry
                    .app_context
                    .as_ref()
                    .and_then(|c| c.window_title.as_ref()),
                entry
                    .app_context
                    .as_ref()
                    .map(|c| format!("{:?}", c.category)),
                entry.created_at.to_rfc3339(),
            ],
        )?;
        debug!("Saved transcription history {}", entry.id);
        Ok(())
    }

    /// Get recent transcription history entries
    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<TranscriptionHistoryEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, status, text, raw_text, error, duration_ms,
                   app_name, bundle_id, window_title, app_category, created_at
            FROM transcription_history
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;

        let entries = stmt
            .query_map([limit as i64], |row| {
                let id: String = row.get(0)?;
                let status_str: String = row.get(1)?;
                let app_name: Option<String> = row.get(6)?;
                let bundle_id: Option<String> = row.get(7)?;
                let window_title: Option<String> = row.get(8)?;
                let app_category_str: Option<String> = row.get(9)?;
                let created_at_str: String = row.get(10)?;

                let app_context = app_name.map(|name| {
                    let category = app_category_str
                        .as_ref()
                        .and_then(|s| parse_app_category(s))
                        .unwrap_or(AppCategory::Unknown);
                    AppContext {
                        app_name: name,
                        bundle_id,
                        window_title,
                        category,
                    }
                });

                let status = match status_str.as_str() {
                    "success" => TranscriptionStatus::Success,
                    "failed" => TranscriptionStatus::Failed,
                    _ => TranscriptionStatus::Failed,
                };

                Ok(TranscriptionHistoryEntry {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    status,
                    text: row.get(2)?,
                    raw_text: row.get(3)?,
                    error: row.get(4)?,
                    duration_ms: row.get::<_, i64>(5)? as u64,
                    app_context,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    // ========== Shortcut methods ==========

    /// Save a shortcut
    pub fn save_shortcut(&self, shortcut: &Shortcut) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO shortcuts (id, trigger, replacement, case_sensitive,
                                              enabled, use_count, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                shortcut.id.to_string(),
                shortcut.trigger,
                shortcut.replacement,
                shortcut.case_sensitive as i32,
                shortcut.enabled as i32,
                shortcut.use_count,
                shortcut.created_at.to_rfc3339(),
                shortcut.updated_at.to_rfc3339(),
            ],
        )?;
        debug!(
            "Saved shortcut {} -> {}",
            shortcut.trigger, shortcut.replacement
        );
        Ok(())
    }

    /// Get all enabled shortcuts
    pub fn get_enabled_shortcuts(&self) -> Result<Vec<Shortcut>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, trigger, replacement, case_sensitive, enabled, use_count, created_at, updated_at
            FROM shortcuts
            WHERE enabled = 1
            ORDER BY trigger
            "#,
        )?;

        let shortcuts = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let created_at_str: String = row.get(6)?;
                let updated_at_str: String = row.get(7)?;

                Ok(Shortcut {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    trigger: row.get(1)?,
                    replacement: row.get(2)?,
                    case_sensitive: row.get::<_, i32>(3)? != 0,
                    enabled: row.get::<_, i32>(4)? != 0,
                    use_count: row.get(5)?,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(shortcuts)
    }

    /// Get all shortcuts (including disabled)
    pub fn get_all_shortcuts(&self) -> Result<Vec<Shortcut>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, trigger, replacement, case_sensitive, enabled, use_count, created_at, updated_at
            FROM shortcuts
            ORDER BY trigger
            "#,
        )?;

        let shortcuts = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let created_at_str: String = row.get(6)?;
                let updated_at_str: String = row.get(7)?;

                Ok(Shortcut {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    trigger: row.get(1)?,
                    replacement: row.get(2)?,
                    case_sensitive: row.get::<_, i32>(3)? != 0,
                    enabled: row.get::<_, i32>(4)? != 0,
                    use_count: row.get(5)?,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(shortcuts)
    }

    /// Increment use count for a shortcut
    pub fn increment_shortcut_use(&self, trigger: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"UPDATE shortcuts SET use_count = use_count + 1, updated_at = ?1 WHERE trigger = ?2"#,
            params![Utc::now().to_rfc3339(), trigger],
        )?;
        Ok(())
    }

    /// Delete a shortcut
    pub fn delete_shortcut(&self, id: &Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM shortcuts WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    // ========== Correction methods ==========

    /// Save or update a correction
    pub fn save_correction(&self, correction: &Correction) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO corrections (id, original, corrected, occurrences, confidence, source, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(original, corrected) DO UPDATE SET
                occurrences = occurrences + 1,
                confidence = ?5,
                updated_at = ?8
            "#,
            params![
                correction.id.to_string(),
                correction.original,
                correction.corrected,
                correction.occurrences,
                correction.confidence,
                format!("{:?}", correction.source),
                correction.created_at.to_rfc3339(),
                correction.updated_at.to_rfc3339(),
            ],
        )?;
        debug!(
            "Saved correction {} -> {}",
            correction.original, correction.corrected
        );
        Ok(())
    }

    /// Get correction for a word if confidence is high enough
    pub fn get_correction(&self, original: &str, min_confidence: f32) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let result: Option<String> = conn
            .query_row(
                r#"
                SELECT corrected FROM corrections
                WHERE original = ?1 AND confidence >= ?2
                ORDER BY confidence DESC
                LIMIT 1
                "#,
                params![original, min_confidence],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    /// Get all corrections above a confidence threshold
    pub fn get_corrections(&self, min_confidence: f32) -> Result<Vec<Correction>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, original, corrected, occurrences, confidence, source, created_at, updated_at
            FROM corrections
            WHERE confidence >= ?1
            ORDER BY confidence DESC
            "#,
        )?;

        let corrections = stmt
            .query_map([min_confidence], |row| {
                let id: String = row.get(0)?;
                let source_str: String = row.get(5)?;
                let created_at_str: String = row.get(6)?;
                let updated_at_str: String = row.get(7)?;

                Ok(Correction {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    original: row.get(1)?,
                    corrected: row.get(2)?,
                    occurrences: row.get(3)?,
                    confidence: row.get(4)?,
                    source: parse_correction_source(&source_str),
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(corrections)
    }

    /// Get all corrections (regardless of confidence)
    pub fn get_all_corrections(&self) -> Result<Vec<Correction>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, original, corrected, occurrences, confidence, source, created_at, updated_at
            FROM corrections
            ORDER BY confidence DESC, occurrences DESC
            "#,
        )?;

        let corrections = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let source_str: String = row.get(5)?;
                let created_at_str: String = row.get(6)?;
                let updated_at_str: String = row.get(7)?;

                Ok(Correction {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    original: row.get(1)?,
                    corrected: row.get(2)?,
                    occurrences: row.get(3)?,
                    confidence: row.get(4)?,
                    source: parse_correction_source(&source_str),
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(corrections)
    }

    /// Delete a correction by ID
    pub fn delete_correction(&self, id: &Uuid) -> Result<bool> {
        let conn = self.conn.lock();
        let rows_affected = conn.execute(
            "DELETE FROM corrections WHERE id = ?1",
            params![id.to_string()],
        )?;
        debug!("Deleted correction {}: {} rows affected", id, rows_affected);
        Ok(rows_affected > 0)
    }

    /// Delete all corrections
    pub fn delete_all_corrections(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let rows_affected = conn.execute("DELETE FROM corrections", [])?;
        debug!("Deleted all corrections: {} rows affected", rows_affected);
        Ok(rows_affected)
    }

    // ========== Analytics event methods ==========

    /// Save an analytics event
    pub fn save_event(&self, event: &AnalyticsEvent) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO events (id, event_type, properties, app_name, bundle_id, window_title, app_category, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                event.id.to_string(),
                format!("{:?}", event.event_type),
                event.properties.to_string(),
                event.app_context.as_ref().map(|c| &c.app_name),
                event
                    .app_context
                    .as_ref()
                    .and_then(|c| c.bundle_id.as_ref()),
                event
                    .app_context
                    .as_ref()
                    .and_then(|c| c.window_title.as_ref()),
                event
                    .app_context
                    .as_ref()
                    .map(|c| format!("{:?}", c.category)),
                event.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get events by type
    pub fn get_events_by_type(
        &self,
        event_type: EventType,
        limit: usize,
    ) -> Result<Vec<AnalyticsEvent>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, event_type, properties, app_name, bundle_id, window_title, app_category, created_at
            FROM events
            WHERE event_type = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let events = stmt
            .query_map(params![format!("{:?}", event_type), limit as i64], |row| {
                let id: String = row.get(0)?;
                let properties_str: String = row.get(2)?;
                let app_name: Option<String> = row.get(3)?;
                let bundle_id: Option<String> = row.get(4)?;
                let window_title: Option<String> = row.get(5)?;
                let app_category_str: Option<String> = row.get(6)?;
                let created_at_str: String = row.get(7)?;

                let app_context = app_name.map(|name| {
                    let category = app_category_str
                        .as_ref()
                        .and_then(|s| parse_app_category(s))
                        .unwrap_or(AppCategory::Unknown);
                    AppContext {
                        app_name: name,
                        bundle_id,
                        window_title,
                        category,
                    }
                });

                Ok(AnalyticsEvent {
                    id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::new_v4()),
                    event_type,
                    properties: serde_json::from_str(&properties_str).unwrap_or_default(),
                    app_context,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }

    // ========== App mode methods ==========

    /// Save app-specific writing mode
    pub fn save_app_mode(&self, app_name: &str, mode: WritingMode) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO app_modes (app_name, writing_mode, updated_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![app_name, format!("{:?}", mode), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Get app-specific writing mode
    pub fn get_app_mode(&self, app_name: &str) -> Result<Option<WritingMode>> {
        let conn = self.conn.lock();
        let result: Option<String> = conn
            .query_row(
                "SELECT writing_mode FROM app_modes WHERE app_name = ?1",
                params![app_name],
                |row| row.get(0),
            )
            .optional()?;

        Ok(result.and_then(|s| parse_writing_mode(&s)))
    }

    // ========== Style sample methods ==========

    /// Save a style sample for learning user's writing style in an app
    pub fn save_style_sample(&self, app_name: &str, sample_text: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO style_samples (id, app_name, sample_text, created_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                Uuid::new_v4().to_string(),
                app_name,
                sample_text,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Get style samples for an app
    pub fn get_style_samples(&self, app_name: &str, limit: usize) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT sample_text FROM style_samples
            WHERE app_name = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let samples = stmt
            .query_map(params![app_name, limit as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(samples)
    }

    // ========== Stats methods ==========

    /// Get total transcription time in milliseconds
    pub fn get_total_transcription_time_ms(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(duration_ms), 0) FROM transcriptions",
            [],
            |row| row.get(0),
        )?;
        Ok(total as u64)
    }

    /// Get transcription count
    pub fn get_transcription_count(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM transcriptions", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Get total word count from saved transcriptions
    pub fn get_total_words_dictated(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT raw_text, processed_text FROM transcriptions")?;
        let rows = stmt.query_map([], |row| {
            let raw_text: String = row.get(0)?;
            let processed_text: String = row.get(1)?;
            Ok((raw_text, processed_text))
        })?;
        let mut total = 0u64;

        for row in rows {
            let (raw_text, processed_text) = row?;
            let text = if raw_text.trim().is_empty() {
                processed_text
            } else {
                raw_text
            };
            total = total.saturating_add(text.split_whitespace().count() as u64);
        }

        Ok(total)
    }
}

fn parse_app_category(s: &str) -> Option<AppCategory> {
    match s {
        "Email" => Some(AppCategory::Email),
        "Slack" => Some(AppCategory::Slack),
        "Code" => Some(AppCategory::Code),
        "Documents" => Some(AppCategory::Documents),
        "Social" => Some(AppCategory::Social),
        "Browser" => Some(AppCategory::Browser),
        "Terminal" => Some(AppCategory::Terminal),
        "Unknown" => Some(AppCategory::Unknown),
        _ => None,
    }
}

fn parse_correction_source(s: &str) -> CorrectionSource {
    match s {
        "UserEdit" => CorrectionSource::UserEdit,
        "ClipboardDiff" => CorrectionSource::ClipboardDiff,
        "Imported" => CorrectionSource::Imported,
        _ => CorrectionSource::UserEdit,
    }
}

fn parse_writing_mode(s: &str) -> Option<WritingMode> {
    match s {
        "Formal" => Some(WritingMode::Formal),
        "Casual" => Some(WritingMode::Casual),
        "VeryCasual" => Some(WritingMode::VeryCasual),
        "Excited" => Some(WritingMode::Excited),
        _ => None,
    }
}

fn parse_contact_category(s: &str) -> ContactCategory {
    match s {
        "Professional" => ContactCategory::Professional,
        "CloseFamily" => ContactCategory::CloseFamily,
        "CasualPeer" => ContactCategory::CasualPeer,
        "Partner" => ContactCategory::Partner,
        "FormalNeutral" => ContactCategory::FormalNeutral,
        _ => ContactCategory::FormalNeutral,
    }
}

fn serialize_contact_category(category: ContactCategory) -> &'static str {
    match category {
        ContactCategory::Professional => "Professional",
        ContactCategory::CloseFamily => "CloseFamily",
        ContactCategory::CasualPeer => "CasualPeer",
        ContactCategory::Partner => "Partner",
        ContactCategory::FormalNeutral => "FormalNeutral",
    }
}

impl Storage {
    // ============ Contact Management ============

    /// Save or update a contact
    pub fn save_contact(&self, contact: &Contact) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute(
            r#"
            INSERT INTO contacts (id, name, organization, category, frequency, last_contacted, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(name) DO UPDATE SET
                organization = excluded.organization,
                category = excluded.category,
                frequency = excluded.frequency,
                last_contacted = excluded.last_contacted,
                updated_at = excluded.updated_at
            "#,
            params![
                contact.id.to_string(),
                contact.name,
                contact.organization,
                serialize_contact_category(contact.category),
                contact.frequency as i64,
                contact.last_contacted.map(|dt| dt.to_rfc3339()),
                contact.created_at.to_rfc3339(),
                contact.updated_at.to_rfc3339(),
            ],
        )?;

        debug!("Saved contact: {}", contact.name);
        Ok(())
    }

    /// Get a contact by name
    pub fn get_contact_by_name(&self, name: &str) -> Result<Option<Contact>> {
        let conn = self.conn.lock();

        let contact = conn
            .query_row(
                "SELECT id, name, organization, category, frequency, last_contacted, created_at, updated_at
                 FROM contacts WHERE name = ?1",
                params![name],
                |row| {
                    let id: String = row.get(0)?;
                    let last_contacted: Option<String> = row.get(5)?;
                    let created_at: String = row.get(6)?;
                    let updated_at: String = row.get(7)?;

                    Ok(Contact {
                        id: Uuid::parse_str(&id).unwrap(),
                        name: row.get(1)?,
                        organization: row.get(2)?,
                        category: parse_contact_category(&row.get::<_, String>(3)?),
                        frequency: row.get::<_, i64>(4)? as u32,
                        last_contacted: last_contacted.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .unwrap()
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&updated_at)
                            .unwrap()
                            .with_timezone(&Utc),
                    })
                },
            )
            .optional()?;

        Ok(contact)
    }

    /// Get all contacts
    pub fn get_all_contacts(&self) -> Result<Vec<Contact>> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, name, organization, category, frequency, last_contacted, created_at, updated_at
             FROM contacts ORDER BY frequency DESC",
        )?;

        let contacts = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let last_contacted: Option<String> = row.get(5)?;
                let created_at: String = row.get(6)?;
                let updated_at: String = row.get(7)?;

                Ok(Contact {
                    id: Uuid::parse_str(&id).unwrap(),
                    name: row.get(1)?,
                    organization: row.get(2)?,
                    category: parse_contact_category(&row.get::<_, String>(3)?),
                    frequency: row.get::<_, i64>(4)? as u32,
                    last_contacted: last_contacted.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .unwrap()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(contacts)
    }

    /// Get top N frequent contacts
    pub fn get_frequent_contacts(&self, limit: usize) -> Result<Vec<Contact>> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, name, organization, category, frequency, last_contacted, created_at, updated_at
             FROM contacts WHERE frequency > 0 ORDER BY frequency DESC LIMIT ?1",
        )?;

        let contacts = stmt
            .query_map(params![limit as i64], |row| {
                let id: String = row.get(0)?;
                let last_contacted: Option<String> = row.get(5)?;
                let created_at: String = row.get(6)?;
                let updated_at: String = row.get(7)?;

                Ok(Contact {
                    id: Uuid::parse_str(&id).unwrap(),
                    name: row.get(1)?,
                    organization: row.get(2)?,
                    category: parse_contact_category(&row.get::<_, String>(3)?),
                    frequency: row.get::<_, i64>(4)? as u32,
                    last_contacted: last_contacted.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .unwrap()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(contacts)
    }

    /// Delete a contact by name
    pub fn delete_contact(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute("DELETE FROM contacts WHERE name = ?1", params![name])?;

        debug!("Deleted contact: {}", name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_crud() {
        let storage = Storage::in_memory().unwrap();

        // test shortcut
        let shortcut = Shortcut::new("my linkedin".to_string(), "jsn.cam/li".to_string());
        storage.save_shortcut(&shortcut).unwrap();

        let shortcuts = storage.get_enabled_shortcuts().unwrap();
        assert_eq!(shortcuts.len(), 1);
        assert_eq!(shortcuts[0].trigger, "my linkedin");
        assert_eq!(shortcuts[0].replacement, "jsn.cam/li");

        // test correction
        let mut correction = Correction::new(
            "teh".to_string(),
            "the".to_string(),
            CorrectionSource::UserEdit,
        );
        correction.update_confidence();
        storage.save_correction(&correction).unwrap();

        let found = storage.get_correction("teh", 0.5).unwrap();
        assert_eq!(found, Some("the".to_string()));

        // test transcription
        let transcription = Transcription::new(
            "hello world".to_string(),
            "Hello world.".to_string(),
            0.95,
            1500,
        );
        storage.save_transcription(&transcription).unwrap();

        let recent = storage.get_recent_transcriptions(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].raw_text, "hello world");
    }

    #[test]
    fn test_app_modes() {
        let storage = Storage::in_memory().unwrap();

        storage.save_app_mode("Slack", WritingMode::Casual).unwrap();
        let mode = storage.get_app_mode("Slack").unwrap();
        assert_eq!(mode, Some(WritingMode::Casual));

        let mode = storage.get_app_mode("Unknown App").unwrap();
        assert_eq!(mode, None);
    }

    #[test]
    fn test_settings_roundtrip() {
        let storage = Storage::in_memory().unwrap();

        storage
            .set_setting(SETTING_OPENAI_API_KEY, "test-key")
            .unwrap();

        let value = storage.get_setting(SETTING_OPENAI_API_KEY).unwrap();
        assert_eq!(value, Some("test-key".to_string()));
    }

    #[test]
    fn test_correction_deletion() {
        let storage = Storage::in_memory().unwrap();

        // Get initial count (includes seeded corrections)
        let initial = storage.get_all_corrections().unwrap();
        let initial_count = initial.len();

        // Create and save multiple corrections
        let correction1 = Correction::new(
            "teh".to_string(),
            "the".to_string(),
            CorrectionSource::UserEdit,
        );
        let correction2 = Correction::new(
            "recieve".to_string(),
            "receive".to_string(),
            CorrectionSource::UserEdit,
        );

        storage.save_correction(&correction1).unwrap();
        storage.save_correction(&correction2).unwrap();

        // Verify both were added
        let all = storage.get_all_corrections().unwrap();
        assert_eq!(all.len(), initial_count + 2);

        // Delete one correction
        let deleted = storage.delete_correction(&correction1.id).unwrap();
        assert!(deleted);

        // Verify one was removed
        let remaining = storage.get_all_corrections().unwrap();
        assert_eq!(remaining.len(), initial_count + 1);
        assert!(remaining.iter().any(|c| c.original == "recieve"));

        // Delete non-existent correction
        let not_deleted = storage.delete_correction(&uuid::Uuid::new_v4()).unwrap();
        assert!(!not_deleted);

        // Delete all corrections
        let deleted_count = storage.delete_all_corrections().unwrap();
        assert_eq!(deleted_count, initial_count + 1);

        // Verify none remain
        let empty = storage.get_all_corrections().unwrap();
        assert!(empty.is_empty());
    }
}
