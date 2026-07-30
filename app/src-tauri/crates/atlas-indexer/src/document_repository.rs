//! `DocumentRepository` interface (§33.2). Implemented by atlas-db.

use atlas_types::document::DocumentRecord;
use atlas_types::ids::{DocumentId, WorkspaceId};
use atlas_utils::AppError;

pub trait DocumentRepository: Send + Sync {
    fn find_by_id(&self, id: DocumentId) -> Result<Option<DocumentRecord>, AppError>;
    fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<DocumentRecord>, AppError>;
    fn upsert(&self, document: DocumentRecord) -> Result<DocumentRecord, AppError>;
    fn delete(&self, id: DocumentId) -> Result<(), AppError>;
}
