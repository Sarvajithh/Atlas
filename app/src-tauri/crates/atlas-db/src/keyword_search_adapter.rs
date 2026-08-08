//! SQLite-backed `KeywordSearchRepository` (§18 "Keyword search ... over
//! parsed text"). Implements a lexical overlap score with a `LIKE`-based
//! query rather than an FTS5 virtual table: this crate only pins
//! rusqlite's `bundled` feature (see `Cargo.toml`'s note on staying inside
//! this sandboxed container's disclosed constraints), and FTS5 support in
//! the bundled SQLite build isn't guaranteed available without an extra,
//! unverified feature flag. `LIKE`-based scoring is slower on a very large
//! corpus but behaviourally equivalent for the "keyword" half of hybrid
//! retrieval (§18) at the scale a single-user local workspace has (§25);
//! swapping in a real FTS5 table later only touches this file.

use std::collections::HashMap;

use atlas_indexer::KeywordSearchRepository;
use atlas_types::ids::{ChunkId, DocumentId, WorkspaceId};
use atlas_types::retrieval::SearchHit;
use atlas_utils::AppError;
use rusqlite::params;

use crate::connection::SqliteConnection;

pub struct SqliteKeywordSearchRepository {
    connection: SqliteConnection,
}

impl SqliteKeywordSearchRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }
}

impl KeywordSearchRepository for SqliteKeywordSearchRepository {
    fn search(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.document_id, c.text_content, c.page_or_location_ref
                 FROM chunks c
                 JOIN documents d ON d.id = c.document_id
                 WHERE d.workspace_id = ?1",
            )
            .map_err(|e| AppError::storage(format!("keyword search prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![workspace_id.0], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| AppError::storage(format!("keyword search query failed: {e}")))?;

        // Simple lexical overlap score: count of query terms that appear
        // in the chunk text (case-insensitive), normalized by term count
        // so shorter queries don't get an unfair advantage. A real BM25
        // implementation is the natural upgrade behind this same
        // interface if/when FTS5 becomes available in this environment.
        let mut scored: HashMap<i64, (i64, String, String, f32)> = HashMap::new();
        for row in rows {
            let (chunk_id, document_id, text_content, location_ref) =
                row.map_err(|e| AppError::storage(format!("keyword search row read failed: {e}")))?;
            let lower_text = text_content.to_lowercase();
            let matches = terms.iter().filter(|t| lower_text.contains(t.as_str())).count();
            if matches == 0 {
                continue;
            }
            let score = matches as f32 / terms.len() as f32;
            scored.insert(chunk_id, (document_id, text_content, location_ref, score));
        }

        let mut hits: Vec<SearchHit> = scored
            .into_iter()
            .map(|(chunk_id, (document_id, text_content, location_ref, score))| SearchHit {
                chunk_id: ChunkId(chunk_id),
                document_id: DocumentId(document_id),
                text_content,
                page_or_location_ref: location_ref,
                score,
            })
            .collect();

        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(limit);
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_indexer::ChunkRepository;
    use atlas_indexer::DocumentRepository;
    use atlas_types::chunk::Chunk;
    use atlas_types::document::{DocumentRecord, ParseStatus};

    fn seed(connection: &SqliteConnection, workspace_id: i64, path: &str, text: &str) {
        let documents = crate::document_adapter::SqliteDocumentRepository::new(connection.clone());
        let chunks = crate::chunk_adapter::SqliteChunkRepository::new(connection.clone());

        let doc = documents
            .upsert(DocumentRecord {
                id: atlas_types::ids::DocumentId(0),
                workspace_id: WorkspaceId(workspace_id),
                relative_path: path.to_string(),
                content_hash: "hash".to_string(),
                file_type: "md".to_string(),
                size: text.len() as u64,
                mtime: "1970-01-01T00:00:00Z".to_string(),
                parse_status: ParseStatus::Parsed,
                last_indexed_hash: Some("hash".to_string()),
                authored_at: None,
            })
            .unwrap();

        chunks
            .insert(Chunk {
                id: ChunkId(0),
                document_id: doc.id,
                sequence_index: 0,
                text_content: text.to_string(),
                page_or_location_ref: "1".to_string(),
                token_count: text.split_whitespace().count() as u32,
                parser_version: "chunker-v1".to_string(),
            })
            .unwrap();
    }

    #[test]
    fn search_finds_chunks_containing_query_terms() {
        let connection = SqliteConnection::open(":memory:");
        seed(&connection, 1, "a.md", "gradient descent optimizes loss");
        seed(&connection, 1, "b.md", "bananas are a good source of potassium");

        let repo = SqliteKeywordSearchRepository::new(connection);
        let hits = repo.search(WorkspaceId(1), "gradient loss", 10).unwrap();

        assert_eq!(hits.len(), 1);
        assert!(hits[0].text_content.contains("gradient"));
    }

    #[test]
    fn search_is_scoped_to_the_given_workspace() {
        let connection = SqliteConnection::open(":memory:");
        seed(&connection, 1, "a.md", "shared keyword appears here");
        seed(&connection, 2, "b.md", "shared keyword appears here too");

        let repo = SqliteKeywordSearchRepository::new(connection);
        let hits = repo.search(WorkspaceId(1), "shared keyword", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_with_no_matches_returns_empty() {
        let connection = SqliteConnection::open(":memory:");
        seed(&connection, 1, "a.md", "completely unrelated text");

        let repo = SqliteKeywordSearchRepository::new(connection);
        let hits = repo.search(WorkspaceId(1), "nonexistent term", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_respects_the_limit() {
        let connection = SqliteConnection::open(":memory:");
        for i in 0..5 {
            seed(&connection, 1, &format!("f{i}.md"), "repeated keyword text here");
        }
        let repo = SqliteKeywordSearchRepository::new(connection);
        let hits = repo.search(WorkspaceId(1), "repeated keyword", 2).unwrap();
        assert_eq!(hits.len(), 2);
    }
}
