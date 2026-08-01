//! Concrete adapters over the embedded local vector store (`store.rs`)
//! implementing every vector-related interface `atlas-indexer` defines:
//! `EmbeddingRepository` (§33.4 relational pointer), `VectorStore` (write
//! path for the actual vector), and `VectorSearchRepository` (read path
//! for nearest-neighbour search, §18). One struct implements all three
//! because they all operate over the same underlying `EmbeddedVectorStore`
//! -- splitting them into separate structs would require sharing the same
//! `Arc<EmbeddedVectorStore>` anyway, with no gain in testability (§46.2:
//! one owner per responsibility, not one *type* per interface).

use std::sync::{Arc, Mutex};

use atlas_indexer::embedding::Embedding;
use atlas_indexer::vector_search::{VectorSearchRepository, VectorStore};
use atlas_indexer::EmbeddingRepository;
use atlas_types::chunk::EmbeddingMetadata;
use atlas_types::ids::{ChunkId, WorkspaceId};
use atlas_types::retrieval::SearchHit;
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;

use crate::store::EmbeddedVectorStore;

/// Backend selection (Qdrant vs. LanceDB vs. this embedded store) is
/// configuration (Governing Principle) at the point a real backend is
/// introduced; this adapter is the currently-wired implementation.
pub struct VectorDbEmbeddingRepository {
    collection_prefix: String,
    store: Arc<EmbeddedVectorStore>,
    /// Relational pointers (§33.4). Kept in-process here rather than in
    /// SQLite for this milestone's default wiring -- `atlas-core` is free
    /// to construct this repository around a `SqliteConnection`-backed
    /// pointer table instead without changing this struct's public shape,
    /// since nothing outside this file reads `pointers` directly.
    pointers: Mutex<Vec<EmbeddingMetadata>>,
}

impl VectorDbEmbeddingRepository {
    pub fn new(collection_prefix: impl Into<String>) -> Self {
        Self::with_store(collection_prefix, Arc::new(EmbeddedVectorStore::in_memory()))
    }

    pub fn with_store(collection_prefix: impl Into<String>, store: Arc<EmbeddedVectorStore>) -> Self {
        Self {
            collection_prefix: collection_prefix.into(),
            store,
            pointers: Mutex::new(Vec::new()),
        }
    }

    pub fn collection_prefix(&self) -> &str {
        &self.collection_prefix
    }

    /// Collection name for a workspace (§22: "Vector DB collections are
    /// namespaced per workspace, so clearing/rebuilding one workspace's
    /// cache never touches another's").
    fn collection_for(&self, workspace_id: WorkspaceId) -> String {
        format!("{}-{}", self.collection_prefix, workspace_id.0)
    }
}

impl EmbeddingRepository for VectorDbEmbeddingRepository {
    fn upsert(&self, metadata: EmbeddingMetadata) -> Result<(), AppError> {
        let mut pointers = self
            .pointers
            .lock()
            .map_err(|_| AppError::vector_storage("embedding pointer lock poisoned"))?;
        if let Some(existing) = pointers.iter_mut().find(|m| m.chunk_id == metadata.chunk_id) {
            *existing = metadata;
        } else {
            pointers.push(metadata);
        }
        Ok(())
    }

    fn find_for_chunk(&self, chunk_id: ChunkId) -> Result<Option<EmbeddingMetadata>, AppError> {
        let pointers = self
            .pointers
            .lock()
            .map_err(|_| AppError::vector_storage("embedding pointer lock poisoned"))?;
        Ok(pointers.iter().find(|m| m.chunk_id == chunk_id).cloned())
    }

    fn delete_for_chunk(&self, chunk_id: ChunkId) -> Result<(), AppError> {
        let mut pointers = self
            .pointers
            .lock()
            .map_err(|_| AppError::vector_storage("embedding pointer lock poisoned"))?;
        pointers.retain(|m| m.chunk_id != chunk_id);
        Ok(())
    }
}

impl VectorStore for VectorDbEmbeddingRepository {
    fn upsert_vector(
        &self,
        workspace_id: WorkspaceId,
        chunk_id: ChunkId,
        vector: Embedding,
    ) -> Result<String, AppError> {
        let collection = self.collection_for(workspace_id);
        let vector_id = self.store.upsert(&collection, chunk_id, vector)?;
        self.upsert(EmbeddingMetadata {
            chunk_id,
            vector_db_collection: collection,
            vector_id: vector_id.clone(),
            embedding_provider_id: "hash-embedding-engine".to_string(),
            created_at: now_iso8601(),
        })?;
        Ok(vector_id)
    }

    fn delete_vector(&self, workspace_id: WorkspaceId, chunk_id: ChunkId) -> Result<(), AppError> {
        let collection = self.collection_for(workspace_id);
        self.store.delete(&collection, chunk_id)?;
        self.delete_for_chunk(chunk_id)
    }
}

impl VectorSearchRepository for VectorDbEmbeddingRepository {
    fn search(
        &self,
        workspace_id: WorkspaceId,
        query_vector: &Embedding,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        let collection = self.collection_for(workspace_id);
        let hits = self.store.search(&collection, query_vector, limit)?;

        let pointers = self
            .pointers
            .lock()
            .map_err(|_| AppError::vector_storage("embedding pointer lock poisoned"))?;

        Ok(hits
            .into_iter()
            .filter_map(|(chunk_id, score)| {
                pointers
                    .iter()
                    .find(|p| p.chunk_id == chunk_id)
                    .map(|_| SearchHit {
                        chunk_id,
                        // document_id/text/location are not known at the
                        // vector-store layer (§18: it only indexes chunk
                        // embeddings); the Retriever (atlas-models) fills
                        // these in from `ChunkRepository` before merging
                        // with keyword results.
                        document_id: atlas_types::ids::DocumentId(0),
                        text_content: String::new(),
                        page_or_location_ref: String::new(),
                        score,
                    })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_vector_stores_both_the_vector_and_its_pointer() {
        let repo = VectorDbEmbeddingRepository::new("workspace");
        let vector_id = repo
            .upsert_vector(WorkspaceId(1), ChunkId(1), vec![1.0, 0.0])
            .unwrap();

        let pointer = repo.find_for_chunk(ChunkId(1)).unwrap().unwrap();
        assert_eq!(pointer.vector_id, vector_id);
        assert_eq!(pointer.vector_db_collection, "workspace-1");
    }

    #[test]
    fn search_returns_hits_with_matching_pointers() {
        let repo = VectorDbEmbeddingRepository::new("workspace");
        repo.upsert_vector(WorkspaceId(1), ChunkId(1), vec![1.0, 0.0]).unwrap();
        repo.upsert_vector(WorkspaceId(1), ChunkId(2), vec![0.0, 1.0]).unwrap();

        let hits = VectorSearchRepository::search(&repo, WorkspaceId(1), &vec![1.0, 0.0], 5).unwrap();
        assert_eq!(hits[0].chunk_id, ChunkId(1));
    }

    #[test]
    fn collections_are_isolated_per_workspace() {
        let repo = VectorDbEmbeddingRepository::new("workspace");
        repo.upsert_vector(WorkspaceId(1), ChunkId(1), vec![1.0, 0.0]).unwrap();

        let hits = VectorSearchRepository::search(&repo, WorkspaceId(2), &vec![1.0, 0.0], 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn delete_vector_removes_both_vector_and_pointer() {
        let repo = VectorDbEmbeddingRepository::new("workspace");
        repo.upsert_vector(WorkspaceId(1), ChunkId(1), vec![1.0, 0.0]).unwrap();
        repo.delete_vector(WorkspaceId(1), ChunkId(1)).unwrap();

        assert!(repo.find_for_chunk(ChunkId(1)).unwrap().is_none());
        let hits = VectorSearchRepository::search(&repo, WorkspaceId(1), &vec![1.0, 0.0], 5).unwrap();
        assert!(hits.is_empty());
    }
}
