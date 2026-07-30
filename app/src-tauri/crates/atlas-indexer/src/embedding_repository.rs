//! `EmbeddingRepository` interface (§33.4). The relational pointer lives in
//! SQLite (atlas-db); the vector itself lives in the Vector DB (atlas-vector,
//! §5). Both are accessed only through this interface.

use atlas_types::chunk::EmbeddingMetadata;
use atlas_types::ids::ChunkId;
use atlas_utils::AppError;

pub trait EmbeddingRepository: Send + Sync {
    fn upsert(&self, metadata: EmbeddingMetadata) -> Result<(), AppError>;
    fn find_for_chunk(&self, chunk_id: ChunkId) -> Result<Option<EmbeddingMetadata>, AppError>;
    fn delete_for_chunk(&self, chunk_id: ChunkId) -> Result<(), AppError>;
}
