//! SQL migration system for Flow database schema management
//!
//! Migrations are embedded at compile time and applied in order.
//! The system tracks applied migrations in a `_migrations` table.

use rusqlite::Connection;
use tracing::{debug, info, warn};

/// Embedded migration files (compiled into binary)
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_initial_schema.sql",
        include_str!("../migrations/001_initial_schema.sql"),
    ),
    (
        "002_add_edit_analytics.sql",
        include_str!("../migrations/002_add_edit_analytics.sql"),
    ),
];

/// Run all pending migrations on the database
pub fn run_migrations(conn: &Connection) -> Result<usize, rusqlite::Error> {
    // Create migrations tracking table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )?;

    // Get list of already-applied migrations
    let applied: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM _migrations ORDER BY id")?;
        stmt.query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut applied_count = 0;

    for (name, sql) in MIGRATIONS {
        if applied.contains(&name.to_string()) {
            debug!("Migration already applied: {}", name);
            continue;
        }

        info!("Applying migration: {}", name);

        // Execute migration SQL
        // Each statement should be idempotent (CREATE IF NOT EXISTS, etc.)
        // We execute batch to handle multiple statements
        match conn.execute_batch(sql) {
            Ok(()) => {
                // Record successful migration
                conn.execute("INSERT INTO _migrations (name) VALUES (?1)", [name])?;
                info!("Successfully applied migration: {}", name);
                applied_count += 1;
            }
            Err(e) => {
                // Some migrations might have ALTER TABLE statements that fail
                // if the column already exists. We handle this gracefully.
                let err_str = e.to_string();
                if err_str.contains("duplicate column name") || err_str.contains("already exists") {
                    warn!(
                        "Migration {} partially applied (some changes already exist): {}",
                        name, e
                    );
                    // Still mark as applied to avoid re-running
                    conn.execute(
                        "INSERT OR IGNORE INTO _migrations (name) VALUES (?1)",
                        [name],
                    )?;
                    applied_count += 1;
                } else {
                    // Real error - propagate
                    return Err(e);
                }
            }
        }
    }

    if applied_count > 0 {
        info!("Applied {} new migration(s)", applied_count);
    } else {
        debug!("Database schema is up to date");
    }

    Ok(applied_count)
}

/// Check if a specific migration has been applied
#[allow(dead_code)]
pub fn is_migration_applied(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM _migrations WHERE name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Get list of all applied migrations
#[allow(dead_code)]
pub fn get_applied_migrations(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT name FROM _migrations ORDER BY id")?;
    stmt.query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice - should not error
        let first = run_migrations(&conn).unwrap();
        let second = run_migrations(&conn).unwrap();

        assert!(first > 0, "First run should apply migrations");
        assert_eq!(second, 0, "Second run should apply nothing (idempotent)");
    }

    #[test]
    fn test_migrations_create_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify core tables exist
        let tables: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"transcriptions".to_string()));
        assert!(tables.contains(&"corrections".to_string()));
        assert!(tables.contains(&"shortcuts".to_string()));
        assert!(tables.contains(&"edit_analytics".to_string()));
        assert!(tables.contains(&"learned_words_sessions".to_string()));
        assert!(tables.contains(&"_migrations".to_string()));
    }

    #[test]
    fn test_applied_migrations_tracked() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let applied = get_applied_migrations(&conn).unwrap();
        assert!(applied.contains(&"001_initial_schema.sql".to_string()));
        assert!(applied.contains(&"002_add_edit_analytics.sql".to_string()));
    }
}
