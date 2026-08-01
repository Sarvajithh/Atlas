//! `VectorSearchRepository` interface (§18 "Vector search (Embedding Engine
//! -> Vector DB) over chunk embeddings"). Kept distinct from
//! `EmbeddingRepository` (which only owns the relational pointer, §33.4):
//! this interface is the read-path query surface a Retriever (§14.1, owned
//! by atlas-models) needs, implemented by atlas-vector against the actual
//! vector store (§5).

use atlas_types::ids::{ChunkId, WorkspaceId};
use atlas_types::retrieval::SearchHit;
use atlas_utils::AppError;

use crate::embedding::Embedding;

/// Write path for the actual vector data (§5, §33.4: "the vector itself
/// lives in the Vector DB, not SQLite"). `EmbeddingRepository` (this crate,
/// `embedding_repository.rs`) only stores the relational *pointer*
/// (`chunk_id` -> `vector_id`); this trait is what the Indexing Pipeline
/// calls to actually persist/replace/remove the float vector behind that
/// pointer, implemented by atlas-vector.
pub trait VectorStore: Send + Sync {
    /// Insert or replace the vector for `chunk_id` within `workspace_id`'s
    /// namespaced collection (§22), returning the vector store's own id
    /// for it (stored onward as `EmbeddingMetadata::vector_id`, §33.4).
    fn upsert_vector(
        &self,
        workspace_id: WorkspaceId,
        chunk_id: ChunkId,
        vector: Embedding,
    ) -> Result<String, AppError>;

    fn delete_vector(&self, workspace_id: WorkspaceId, chunk_id: ChunkId) -> Result<(), AppError>;
}

pub trait VectorSearchRepository: Send + Sync {
    /// Nearest-neighbour search by cosine similarity over every chunk
    /// embedding stored for `workspace_id`'s vector collection (§22:
    /// "Vector DB collections are namespaced per workspace"). Returns up
    /// to `limit` hits, highest similarity first.
    fn search(
        &self,
        workspace_id: WorkspaceId,
        query_vector: &Embedding,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError>;
}
