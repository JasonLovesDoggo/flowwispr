-- Edit analytics for tracking patterns and alignment data

-- Edit analytics table - stores alignment results for analysis
CREATE TABLE IF NOT EXISTS edit_analytics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transcript_id TEXT,
    word_edit_vector TEXT NOT NULL,
    punct_edit_vector TEXT,
    original_text TEXT,
    edited_text TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_edit_analytics_transcript ON edit_analytics(transcript_id);
CREATE INDEX IF NOT EXISTS idx_edit_analytics_created ON edit_analytics(created_at);

-- Track newly learned words for undo functionality
CREATE TABLE IF NOT EXISTS learned_words_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    words TEXT NOT NULL,  -- JSON array of words
    can_undo INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_learned_words_created ON learned_words_sessions(created_at);

-- Add observed_source column to corrections if it doesn't exist
-- This tracks what word the correction was observed correcting FROM
-- (allows for more nuanced learning)
ALTER TABLE corrections ADD COLUMN observed_source TEXT;
