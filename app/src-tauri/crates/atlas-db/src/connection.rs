//! Shared SQLite connection handle (§41 step 3: "Open Database"). All
//! adapters in this crate operate over one `SqliteConnection`, wired in by
//! atlas-core at startup.
//!
//! `rusqlite::Connection` is `!Sync`, so every adapter shares a single
//! connection through a `Mutex`, matching §13's "SQLite is the source of
//! truth for structured state" -- one writer at a time, consistent with
//! SQLite's own concurrency model for a single-user, local-first app (§2.1).

use std::sync::{Arc, Mutex, MutexGuard};

use atlas_utils::AppError;

use crate::migrations::run_migrations;

#[derive(Clone)]
pub struct SqliteConnection {
    database_path: String,
    inner: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteConnection {
    /// Open (or create) the database at `database_path` and run any
    /// pending migrations (§41 step 3). `":memory:"` opens a private,
    /// in-memory database -- used throughout this workspace's tests so no
    /// crate's test suite touches the filesystem.
    pub fn open(database_path: impl Into<String>) -> Self {
        let database_path = database_path.into();
        let conn = rusqlite::Connection::open(&database_path)
            .unwrap_or_else(|e| panic!("failed to open SQLite database '{database_path}': {e}"));
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("failed to enable foreign_keys pragma");
        run_migrations(&conn).expect("failed to run pending migrations");
        Self {
            database_path,
            inner: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn database_path(&self) -> &str {
        &self.database_path
    }

    /// Lock the underlying connection for a single unit of work. Poisoned
    /// locks are surfaced as a structured storage error (§45.2: no failure
    /// is ever silently swallowed) rather than panicking the caller.
    pub fn lock(&self) -> Result<MutexGuard<'_, rusqlite::Connection>, AppError> {
        self.inner
            .lock()
            .map_err(|_| AppError::storage("database connection lock poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_runs_migrations_without_error() {
        let conn = SqliteConnection::open(":memory:");
        assert_eq!(conn.database_path(), ":memory:");
        // The `workspaces` table must exist after migrations run.
        let guard = conn.lock().unwrap();
        let count: i64 = guard
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'workspaces'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn opening_twice_at_the_same_path_is_idempotent() {
        let dir = std::env::temp_dir().join(format!(
            "atlas-db-conn-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let _first = SqliteConnection::open(&path);
        let _second = SqliteConnection::open(&path);

        let _ = std::fs::remove_file(&path);
    }
}
