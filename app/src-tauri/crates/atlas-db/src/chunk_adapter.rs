//! SQLite-backed `ChunkRepository` (§33.3).

use atlas_indexer::ChunkRepository;
use atlas_types::chunk::Chunk;
use atlas_types::ids::{ChunkId, DocumentId};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

use crate::connection::SqliteConnection;

pub struct SqliteChunkRepository {
    connection: SqliteConnection,
}

impl SqliteChunkRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

type ChunkRow = (i64, i64, i64, String, String, i64, String);

fn row_to_chunk(row: &Row<'_>) -> rusqlite::Result<ChunkRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn tuple_to_chunk(tuple: ChunkRow) -> Chunk {
    let (id, document_id, sequence_index, text_content, page_or_location_ref, token_count, parser_version) =
        tuple;
    Chunk {
        id: ChunkId(id),
        document_id: DocumentId(document_id),
        sequence_index: sequence_index as u32,
        text_content,
        page_or_location_ref,
        token_count: token_count as u32,
        parser_version,
    }
}

const SELECT_COLUMNS: &str =
    "id, document_id, sequence_index, text_content, page_or_location_ref, token_count, parser_version FROM chunks";

impl ChunkRepository for SqliteChunkRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Chunk>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} WHERE document_id = ?1 ORDER BY sequence_index ASC"
            ))
            .map_err(|e| AppError::storage(format!("chunk list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![document_id.0], row_to_chunk)
            .map_err(|e| AppError::storage(format!("chunk list query failed: {e}")))?;

        let mut chunks = Vec::new();
        for row in rows {
            chunks.push(tuple_to_chunk(
                row.map_err(|e| AppError::storage(format!("chunk row read failed: {e}")))?,
            ));
        }
        Ok(chunks)
    }

    fn insert(&self, chunk: Chunk) -> Result<Chunk, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO chunks
                (document_id, sequence_index, text_content, page_or_location_ref, token_count, parser_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                chunk.document_id.0,
                chunk.sequence_index,
                chunk.text_content,
                chunk.page_or_location_ref,
                chunk.token_count,
                chunk.parser_version,
            ],
        )
        .map_err(|e| AppError::storage(format!("chunk insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(Chunk {
            id: ChunkId(id),
            ..chunk
        })
    }

    fn delete_for_document(&self, document_id: DocumentId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            params![document_id.0],
        )
        .map_err(|e| AppError::storage(format!("chunk delete_for_document failed: {e}")))?;
        Ok(())
    }

    fn find_by_id(&self, id: ChunkId) -> Result<Option<Chunk>, AppError> {
        let conn = self.connection.lock()?;
        conn.query_row(
            &format!("SELECT {SELECT_COLUMNS} WHERE id = ?1"),
            params![id.0],
            row_to_chunk,
        )
        .optional()
        .map_err(|e| AppError::storage(format!("chunk find_by_id failed: {e}")))
        .map(|opt| opt.map(tuple_to_chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(document_id: i64, sequence_index: u32, text: &str) -> Chunk {
        Chunk {
            id: ChunkId(0),
            document_id: DocumentId(document_id),
            sequence_index,
            text_content: text.to_string(),
            page_or_location_ref: "1".to_string(),
            token_count: text.split_whitespace().count() as u32,
            parser_version: "chunker-v1".to_string(),
        }
    }

    fn repo() -> SqliteChunkRepository {
        SqliteChunkRepository::new(SqliteConnection::open(":memory:"))
    }

    #[test]
    fn insert_assigns_an_id_and_persists_the_row() {
        let repo = repo();
        let inserted = repo.insert(sample(1, 0, "hello world")).unwrap();
        assert_ne!(inserted.id.0, 0);
        assert!(repo.find_by_id(inserted.id).unwrap().is_some());
    }

    #[test]
    fn list_for_document_returns_chunks_in_sequence_order() {
        let repo = repo();
        repo.insert(sample(1, 1, "second")).unwrap();
        repo.insert(sample(1, 0, "first")).unwrap();
        repo.insert(sample(2, 0, "other document")).unwrap();

        let chunks = repo.list_for_document(DocumentId(1)).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].text_content, "first");
        assert_eq!(chunks[1].text_content, "second");
    }

    #[test]
    fn delete_for_document_only_removes_that_documents_chunks() {
        let repo = repo();
        repo.insert(sample(1, 0, "keep me? no")).unwrap();
        repo.insert(sample(2, 0, "survivor")).unwrap();

        repo.delete_for_document(DocumentId(1)).unwrap();

        assert!(repo.list_for_document(DocumentId(1)).unwrap().is_empty());
        assert_eq!(repo.list_for_document(DocumentId(2)).unwrap().len(), 1);
    }

    #[test]
    fn find_by_id_missing_returns_none() {
        let repo = repo();
        assert!(repo.find_by_id(ChunkId(999)).unwrap().is_none());
    }
}
