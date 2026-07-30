//! SQLite-backed `DocumentRepository` (§33.2). Owned conceptually by
//! `core-indexing`/atlas-indexer; implemented here per Dependency Inversion.

use atlas_indexer::DocumentRepository;
use atlas_types::document::DocumentRecord;
use atlas_types::ids::{DocumentId, WorkspaceId};
use atlas_utils::AppError;

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

impl DocumentRepository for SqliteDocumentRepository {
    fn find_by_id(&self, _id: DocumentId) -> Result<Option<DocumentRecord>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn list_for_workspace(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<DocumentRecord>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn upsert(&self, _document: DocumentRecord) -> Result<DocumentRecord, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete(&self, _id: DocumentId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
