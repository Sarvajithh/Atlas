//! Concrete `EmbeddingRepository` adapter over the local vector store
//! (§5, §33.4). Backend selection (Qdrant vs. LanceDB) is configuration
//! (Governing Principle), not hardcoded here.

use atlas_indexer::EmbeddingRepository;
use atlas_types::chunk::EmbeddingMetadata;
use atlas_types::ids::ChunkId;
use atlas_utils::AppError;

pub struct VectorDbEmbeddingRepository {
    /// Connection/collection-namespace details are deferred to a future
    /// milestone; this field anchors where that configuration will live.
    collection_prefix: String,
}

impl VectorDbEmbeddingRepository {
    pub fn new(collection_prefix: impl Into<String>) -> Self {
        Self {
            collection_prefix: collection_prefix.into(),
        }
    }

    pub fn collection_prefix(&self) -> &str {
        &self.collection_prefix
    }
}

impl EmbeddingRepository for VectorDbEmbeddingRepository {
    fn upsert(&self, _metadata: EmbeddingMetadata) -> Result<(), AppError> {
        unimplemented!("vector store write path is out of scope for this milestone")
    }

    fn find_for_chunk(&self, _chunk_id: ChunkId) -> Result<Option<EmbeddingMetadata>, AppError> {
        unimplemented!("vector store read path is out of scope for this milestone")
    }

    fn delete_for_chunk(&self, _chunk_id: ChunkId) -> Result<(), AppError> {
        unimplemented!("vector store delete path is out of scope for this milestone")
    }
}
