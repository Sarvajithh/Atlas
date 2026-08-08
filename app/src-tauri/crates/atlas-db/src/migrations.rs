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
    // §33.2 `documents`, §33.3 `chunks`, §33.4 `embeddings_metadata` -- the
    // three tables the Knowledge Engine milestone (Phase 3: Document
    // Abstraction Layer, Parser Framework, Chunking Engine, Embedding
    // Engine) owns and reads/writes through `DocumentRepository`/
    // `ChunkRepository`/`EmbeddingRepository` (atlas-indexer, §14).
    Migration {
        id: "0004_create_documents",
        sql: "
            CREATE TABLE IF NOT EXISTS documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                file_type TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime TEXT NOT NULL,
                parse_status TEXT NOT NULL,
                last_indexed_hash TEXT,
                UNIQUE(workspace_id, relative_path)
            );
            CREATE INDEX IF NOT EXISTS idx_documents_workspace ON documents(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_documents_parse_status ON documents(parse_status);
        ",
    },
    Migration {
        id: "0005_create_chunks",
        sql: "
            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL,
                sequence_index INTEGER NOT NULL,
                text_content TEXT NOT NULL,
                page_or_location_ref TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                parser_version TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);
        ",
    },
    Migration {
        id: "0006_create_embeddings_metadata",
        sql: "
            CREATE TABLE IF NOT EXISTS embeddings_metadata (
                chunk_id INTEGER PRIMARY KEY,
                vector_db_collection TEXT NOT NULL,
                vector_id TEXT NOT NULL,
                embedding_provider_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_collection ON embeddings_metadata(vector_db_collection);
        ",
    },
    // Phase 4 (§14.1 Engines Module, §19 Student Memory): `model_registry`
    // (§33.13, owned by atlas-models), the `chat_sessions`/`chat_messages`
    // pair (§33.10/§33.11, Conversation Memory), and the remaining
    // `student_memory` group tables (§33.7-§33.11, §33.16-§33.18) owned by
    // atlas-memory. `concept_node_id` columns below intentionally carry no
    // FOREIGN KEY constraint, matching the existing `jobs`/`events`
    // convention (§33.14/§33.15: "loosely references... by id, not by hard
    // foreign key") -- the Concept Graph milestone that owns `concept_nodes`
    // is out of scope for this milestone.
    Migration {
        id: "0007_create_model_registry",
        sql: "
            CREATE TABLE IF NOT EXISTS model_registry (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                model_identifier TEXT NOT NULL,
                engine_role TEXT NOT NULL,
                capabilities TEXT NOT NULL,
                context_length INTEGER NOT NULL,
                vram_requirement INTEGER,
                status TEXT NOT NULL,
                version TEXT NOT NULL,
                supported_tasks TEXT NOT NULL,
                is_selected_for_role INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_model_registry_role ON model_registry(engine_role);
            CREATE INDEX IF NOT EXISTS idx_model_registry_status ON model_registry(status);
        ",
    },
    Migration {
        id: "0008_create_chat_sessions",
        sql: "
            CREATE TABLE IF NOT EXISTS chat_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                document_id INTEGER,
                title TEXT NOT NULL,
                mode TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_workspace ON chat_sessions(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_document ON chat_sessions(document_id);
        ",
    },
    Migration {
        id: "0009_create_chat_messages",
        sql: "
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                engine_pipeline_used TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_session_created ON chat_messages(session_id, created_at);
        ",
    },
    Migration {
        id: "0010_create_annotations",
        sql: "
            CREATE TABLE IF NOT EXISTS annotations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL,
                location_ref TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_annotations_document ON annotations(document_id);
        ",
    },
    Migration {
        id: "0011_create_bookmarks",
        sql: "
            CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL,
                location_ref TEXT NOT NULL,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_document ON bookmarks(document_id);
        ",
    },
    Migration {
        id: "0012_create_learning_progress",
        sql: "
            CREATE TABLE IF NOT EXISTS learning_progress (
                concept_node_id INTEGER PRIMARY KEY,
                mastery_score REAL NOT NULL,
                weakness_score REAL NOT NULL,
                last_reviewed_at TEXT,
                attempt_count INTEGER NOT NULL
            );
        ",
    },
    Migration {
        id: "0013_create_revision_history",
        sql: "
            CREATE TABLE IF NOT EXISTS revision_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                concept_node_id INTEGER NOT NULL,
                scheduled_at TEXT NOT NULL,
                completed_at TEXT,
                outcome TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_revision_history_concept ON revision_history(concept_node_id);
            CREATE INDEX IF NOT EXISTS idx_revision_history_scheduled ON revision_history(scheduled_at);
        ",
    },
    Migration {
        id: "0014_create_analytics",
        sql: "
            CREATE TABLE IF NOT EXISTS analytics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                metric_key TEXT NOT NULL,
                metric_value REAL NOT NULL,
                computed_at TEXT NOT NULL,
                period TEXT NOT NULL,
                UNIQUE(workspace_id, metric_key, period)
            );
            CREATE INDEX IF NOT EXISTS idx_analytics_workspace_metric_period ON analytics(workspace_id, metric_key, period);
        ",
    },
    Migration {
        id: "0015_create_settings",
        sql: "
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                value_type TEXT NOT NULL,
                scope TEXT NOT NULL,
                workspace_id INTEGER,
                updated_at TEXT NOT NULL,
                UNIQUE(key, scope, workspace_id)
            );
            CREATE INDEX IF NOT EXISTS idx_settings_key_scope ON settings(key, scope, workspace_id);
        ",
    },
    // Concept Graph (§20, §33.5, §33.6), owned by core-graph / atlas-graph.
    // This was previously missing entirely -- `SqliteGraphRepository`
    // (atlas-db::graph_adapter) was wired live into `AppFacade` (Fix 1,
    // P0 audit) against tables that were never created, so every query
    // would have failed with "no such table" even once the
    // `unimplemented!()` stubs were replaced with real SQL. `concept_edges`
    // rows loosely reference `concept_nodes` by id (no FOREIGN KEY),
    // matching the existing `jobs`/`events`/`learning_progress` convention
    // elsewhere in this file.
    Migration {
        id: "0016_create_concept_graph",
        sql: "
            CREATE TABLE IF NOT EXISTS concept_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                label TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_concept_nodes_workspace ON concept_nodes(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_concept_nodes_label ON concept_nodes(label);

            CREATE TABLE IF NOT EXISTS concept_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_node_id INTEGER NOT NULL,
                to_node_id INTEGER NOT NULL,
                relation_type TEXT NOT NULL,
                weight REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_concept_edges_from ON concept_edges(from_node_id);
            CREATE INDEX IF NOT EXISTS idx_concept_edges_to ON concept_edges(to_node_id);
        ",
    },
    // Concept node provenance (§20 Research Mode phase): which document(s)
    // a given concept node was actually extracted from. Added because
    // Research Mode's Citation Graph needs to tell a *cross-document*
    // relationship (the same concept, or a related pair of concepts,
    // showing up in more than one source document) apart from a purely
    // within-one-document relationship -- and `concept_nodes` itself is
    // workspace-scoped, not document-scoped (a node is one concept shared
    // across however many documents mention it, by design, so extraction
    // can dedup a concept re-mentioned across sources instead of creating
    // a duplicate node per document). This is a new join table recording
    // provenance, not a restructuring of the existing node/edge schema --
    // `concept_nodes`/`concept_edges` themselves are unchanged (per the
    // architecture contract's node/edge model being frozen for this
    // phase).
    Migration {
        id: "0017_create_concept_node_sources",
        sql: "
            CREATE TABLE IF NOT EXISTS concept_node_sources (
                node_id INTEGER NOT NULL,
                document_id INTEGER NOT NULL,
                PRIMARY KEY (node_id, document_id)
            );
            CREATE INDEX IF NOT EXISTS idx_concept_node_sources_node ON concept_node_sources(node_id);
            CREATE INDEX IF NOT EXISTS idx_concept_node_sources_document ON concept_node_sources(document_id);
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

        for table in [
            "workspaces",
            "events",
            "jobs",
            "documents",
            "chunks",
            "embeddings_metadata",
            "model_registry",
            "chat_sessions",
            "chat_messages",
            "annotations",
            "bookmarks",
            "learning_progress",
            "revision_history",
            "analytics",
            "settings",
        ] {
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
