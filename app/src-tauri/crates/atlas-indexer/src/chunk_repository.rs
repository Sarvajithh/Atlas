//! `ChunkRepository` interface (§33.3). Implemented by atlas-db.
//! Embedding pointers (§33.4) are owned by atlas-vector's repository, kept
//! separate here per §16's module-boundary rule.

use atlas_types::chunk::Chunk;
use atlas_types::ids::{ChunkId, DocumentId};
use atlas_utils::AppError;

pub trait ChunkRepository: Send + Sync {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Chunk>, AppError>;
    fn insert(&self, chunk: Chunk) -> Result<Chunk, AppError>;
    fn delete_for_document(&self, document_id: DocumentId) -> Result<(), AppError>;
    fn find_by_id(&self, id: ChunkId) -> Result<Option<Chunk>, AppError>;
}
