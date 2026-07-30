//! `BookmarkRepository` interface (§33.9). Implemented by atlas-db.

use atlas_types::ids::{BookmarkId, DocumentId};
use atlas_types::memory::Bookmark;
use atlas_utils::AppError;

pub trait BookmarkRepository: Send + Sync {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Bookmark>, AppError>;
    fn insert(&self, bookmark: Bookmark) -> Result<Bookmark, AppError>;
    fn delete(&self, id: BookmarkId) -> Result<(), AppError>;
}
