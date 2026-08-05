//! SQLite-backed `DocumentRepository` (§33.2). Owned conceptually by
//! `core-indexing`/atlas-indexer; implemented here per Dependency Inversion.

use atlas_indexer::DocumentRepository;
use atlas_types::document::{DocumentRecord, ParseStatus};
use atlas_types::ids::{DocumentId, WorkspaceId};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

use crate::connection::SqliteConnection;

pub struct SqliteDocumentRepository {
    connection: SqliteConnection,
}

impl SqliteDocumentRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

fn status_to_str(status: &ParseStatus) -> &'static str {
    match status {
        ParseStatus::Pending => "pending",
        ParseStatus::Parsing => "parsing",
        ParseStatus::Parsed => "parsed",
        ParseStatus::ParsedEmpty => "parsed_empty",
        ParseStatus::Failed => "failed",
    }
}

fn status_from_str(value: &str) -> Result<ParseStatus, AppError> {
    match value {
        "pending" => Ok(ParseStatus::Pending),
        "parsing" => Ok(ParseStatus::Parsing),
        "parsed" => Ok(ParseStatus::Parsed),
        "parsed_empty" => Ok(ParseStatus::ParsedEmpty),
        "failed" => Ok(ParseStatus::Failed),
        other => Err(AppError::storage(format!(
            "unrecognized parse_status in database: {other}"
        ))),
    }
}

#[allow(clippy::type_complexity)]
type DocumentRow = (
    i64,
    i64,
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
);

fn row_to_document(row: &Row<'_>) -> rusqlite::Result<DocumentRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn tuple_to_document(tuple: DocumentRow) -> Result<DocumentRecord, AppError> {
    let (id, workspace_id, relative_path, content_hash, file_type, size, mtime, parse_status, last_indexed_hash) =
        tuple;
    Ok(DocumentRecord {
        id: DocumentId(id),
        workspace_id: WorkspaceId(workspace_id),
        relative_path,
        content_hash,
        file_type,
        size: size as u64,
        mtime,
        parse_status: status_from_str(&parse_status)?,
        last_indexed_hash,
    })
}

const SELECT_COLUMNS: &str = "id, workspace_id, relative_path, content_hash, file_type, size, mtime, parse_status, last_indexed_hash FROM documents";

impl DocumentRepository for SqliteDocumentRepository {
    fn find_by_id(&self, id: DocumentId) -> Result<Option<DocumentRecord>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} WHERE id = ?1"),
                params![id.0],
                row_to_document,
            )
            .optional()
            .map_err(|e| AppError::storage(format!("document find_by_id failed: {e}")))?;
        result.map(tuple_to_document).transpose()
    }

    fn list_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<DocumentRecord>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} WHERE workspace_id = ?1 ORDER BY id ASC"
            ))
            .map_err(|e| AppError::storage(format!("document list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], row_to_document)
            .map_err(|e| AppError::storage(format!("document list query failed: {e}")))?;

        let mut documents = Vec::new();
        for row in rows {
            let tuple = row.map_err(|e| AppError::storage(format!("document row read failed: {e}")))?;
            documents.push(tuple_to_document(tuple)?);
        }
        Ok(documents)
    }

    /// Insert a new document row, or update the existing one for the same
    /// `(workspace_id, relative_path)` (§22: a document is re-indexed in
    /// place, not duplicated, when its path is seen again with a new
    /// content hash).
    fn upsert(&self, document: DocumentRecord) -> Result<DocumentRecord, AppError> {
        let conn = self.connection.lock()?;

        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM documents WHERE workspace_id = ?1 AND relative_path = ?2",
                params![document.workspace_id.0, document.relative_path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::storage(format!("document upsert lookup failed: {e}")))?;

        if let Some(id) = existing_id {
            conn.execute(
                "UPDATE documents
                 SET content_hash = ?1, file_type = ?2, size = ?3, mtime = ?4,
                     parse_status = ?5, last_indexed_hash = ?6
                 WHERE id = ?7",
                params![
                    document.content_hash,
                    document.file_type,
                    document.size as i64,
                    document.mtime,
                    status_to_str(&document.parse_status),
                    document.last_indexed_hash,
                    id,
                ],
            )
            .map_err(|e| AppError::storage(format!("document update failed: {e}")))?;
            Ok(DocumentRecord {
                id: DocumentId(id),
                ..document
            })
        } else {
            conn.execute(
                "INSERT INTO documents
                    (workspace_id, relative_path, content_hash, file_type, size, mtime, parse_status, last_indexed_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    document.workspace_id.0,
                    document.relative_path,
                    document.content_hash,
                    document.file_type,
                    document.size as i64,
                    document.mtime,
                    status_to_str(&document.parse_status),
                    document.last_indexed_hash,
                ],
            )
            .map_err(|e| AppError::storage(format!("document insert failed: {e}")))?;
            let id = conn.last_insert_rowid();
            Ok(DocumentRecord {
                id: DocumentId(id),
                ..document
            })
        }
    }

    fn delete(&self, id: DocumentId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute("DELETE FROM documents WHERE id = ?1", params![id.0])
            .map_err(|e| AppError::storage(format!("document delete failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(workspace_id: i64, relative_path: &str) -> DocumentRecord {
        DocumentRecord {
            id: DocumentId(0),
            workspace_id: WorkspaceId(workspace_id),
            relative_path: relative_path.to_string(),
            content_hash: "hash-a".to_string(),
            file_type: "md".to_string(),
            size: 100,
            mtime: "1970-01-01T00:00:00Z".to_string(),
            parse_status: ParseStatus::Pending,
            last_indexed_hash: None,
        }
    }

    fn repo() -> SqliteDocumentRepository {
        SqliteDocumentRepository::new(SqliteConnection::open(":memory:"))
    }

    #[test]
    fn upsert_inserts_a_new_document() {
        let repo = repo();
        let inserted = repo.upsert(sample(1, "notes.md")).unwrap();
        assert_ne!(inserted.id.0, 0);
        assert!(repo.find_by_id(inserted.id).unwrap().is_some());
    }

    #[test]
    fn upsert_on_same_path_updates_in_place_rather_than_duplicating() {
        let repo = repo();
        let first = repo.upsert(sample(1, "notes.md")).unwrap();

        let mut changed = sample(1, "notes.md");
        changed.content_hash = "hash-b".to_string();
        changed.parse_status = ParseStatus::Parsed;
        let second = repo.upsert(changed).unwrap();

        assert_eq!(first.id, second.id);
        let all = repo.list_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content_hash, "hash-b");
        assert_eq!(all[0].parse_status, ParseStatus::Parsed);
    }

    #[test]
    fn list_for_workspace_only_returns_that_workspaces_documents() {
        let repo = repo();
        repo.upsert(sample(1, "a.md")).unwrap();
        repo.upsert(sample(2, "b.md")).unwrap();

        let ws1 = repo.list_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(ws1.len(), 1);
        assert_eq!(ws1[0].relative_path, "a.md");
    }

    #[test]
    fn delete_removes_the_row() {
        let repo = repo();
        let inserted = repo.upsert(sample(1, "gone.md")).unwrap();
        repo.delete(inserted.id).unwrap();
        assert!(repo.find_by_id(inserted.id).unwrap().is_none());
    }

    #[test]
    fn find_by_id_missing_returns_none() {
        let repo = repo();
        assert!(repo.find_by_id(DocumentId(999)).unwrap().is_none());
    }
}
