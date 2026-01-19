-- Flow Core Initial Schema
-- This migration establishes the base schema.
-- Note: Tables may already exist from inline schema, so we use IF NOT EXISTS.

-- Transcriptions table
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
CREATE INDEX IF NOT EXISTS idx_transcriptions_created ON transcriptions(created_at);

-- Transcription history for tracking success/failure
CREATE TABLE IF NOT EXISTS transcription_history (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    text TEXT NOT NULL,
    raw_text TEXT NOT NULL DEFAULT '',
    error TEXT,
    duration_ms INTEGER NOT NULL,
    app_name TEXT,
    bundle_id TEXT,
    window_title TEXT,
    app_category TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_transcription_history_created ON transcription_history(created_at);

-- Shortcuts (voice triggers)
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
CREATE INDEX IF NOT EXISTS idx_shortcuts_trigger ON shortcuts(trigger);

-- Corrections (learned typo fixes)
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
CREATE INDEX IF NOT EXISTS idx_corrections_original ON corrections(original);
CREATE INDEX IF NOT EXISTS idx_corrections_confidence ON corrections(confidence DESC);

-- Analytics events
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
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);

-- App-specific writing modes
CREATE TABLE IF NOT EXISTS app_modes (
    app_name TEXT PRIMARY KEY,
    writing_mode TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Style samples for learning user writing style
CREATE TABLE IF NOT EXISTS style_samples (
    id TEXT PRIMARY KEY,
    app_name TEXT NOT NULL,
    sample_text TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_style_samples_app ON style_samples(app_name);

-- Settings (key-value store)
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Contacts for adaptive writing modes
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
CREATE INDEX IF NOT EXISTS idx_contacts_name ON contacts(name);
CREATE INDEX IF NOT EXISTS idx_contacts_frequency ON contacts(frequency DESC);
