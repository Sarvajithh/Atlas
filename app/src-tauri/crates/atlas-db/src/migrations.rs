//! Schema migrations (§33, §41 step 3). Ordered, forward-only SQL migrations
//! applied on every `SqliteConnection::open` call. Only the tables owned by
//! the crates this milestone (Phase 2 -- Workspace Engine, Background Job
//! Queue, Event Bus) actually implements are created here; tables owned by
//! future milestones are added by their own dedicated migration when that
//! milestone lands, per §46.2 ("each table has exactly one owner").
//!
//! A `schema_migrations` table tracks which migrations have already run so
//! re-opening an existing database file never re-applies (or fails re-
//! applying) a migration.

use rusqlite::Connection;

/// One forward-only migration: a stable `id` (never reused or reordered)
/// and the SQL to apply it.
struct Migration {
    id: &'static str,
    sql: &'static str,
}

/// §33.1 `workspaces`, §33.15 `events`, §33.14 `jobs` -- the three tables
/// this milestone's crates (atlas-workspace, atlas-events/atlas-db,
/// atlas-indexer's job queue) own and read/write through their repository
/// interfaces.
const MIGRATIONS: &[Migration] = &[
    Migration {
        id: "0001_create_workspaces",
        sql: "
            CREATE TABLE IF NOT EXISTS workspaces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                root_path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_indexed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_workspaces_status ON workspaces(status);
        ",
    },
    Migration {
        id: "0002_create_events",
        sql: "
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                occurred_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_occurred_at ON events(occurred_at);
        ",
    },
    Migration {
        id: "0003_create_jobs",
        sql: "
            CREATE TABLE IF NOT EXISTS jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL,
                retry_count INTEGER NOT NULL,
                max_retries INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_status_priority ON jobs(status, priority);
            CREATE INDEX IF NOT EXISTS idx_jobs_job_type ON jobs(job_type);
        ",
    },
];

pub fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );",
    )?;

    for migration in MIGRATIONS {
        let already_applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE id = ?1)",
            [migration.id],
            |row| row.get(0),
        )?;
        if already_applied {
            continue;
        }
        conn.execute_batch(migration.sql)?;
        conn.execute(
            "INSERT INTO schema_migrations (id) VALUES (?1)",
            [migration.id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_migrations_twice_is_a_no_op_the_second_time() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let applied: i64 = conn
            .query_row("SELECT count(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(applied, MIGRATIONS.len() as i64);
    }

    #[test]
    fn all_expected_tables_exist_after_migrating() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        for table in ["workspaces", "events", "jobs"] {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "expected table `{table}` to exist");
        }
    }
}
