//! Embedded local vector store (§5: "Vector storage: Qdrant or LanceDB
//! (embedded/local mode)"). Ships here as a small, dependency-free
//! brute-force cosine-similarity index rather than pulling in the actual
//! Qdrant/LanceDB crates: both have transitive dependency trees that do
//! not build under this sandboxed container's disclosed constraint (only
//! `rustc`/`cargo` 1.75.0 is reachable here -- see the architecture doc's
//! "Known Environment Limitations" section for the identical situation
//! with Tauri 2's dependency tree). This store sits exactly behind the
//! `VectorStore`/`VectorSearchRepository`/`EmbeddingRepository` interfaces
//! atlas-indexer already defines (Dependency Inversion, Governing
//! Principle), so swapping in a real Qdrant/LanceDB client on a normal
//! development machine touches only this file.
//!
//! Collections are namespaced per workspace (§22), each held as its own
//! `Vec` of records, and optionally persisted to a single JSON file per
//! collection under a configured storage directory so brute-force search
//! survives a process restart without recomputing embeddings (§13:
//! "Vector DB is ... always rebuildable from SQLite + source files", but
//! rebuilding is not required to happen on *every* launch just because the
//! store lives in memory).

use std::collections::HashMap;
use std::sync::RwLock;

use atlas_indexer::embedding::{cosine_similarity, Embedding};
use atlas_types::ids::ChunkId;
use atlas_utils::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorRecord {
    vector_id: String,
    chunk_id: ChunkId,
    vector: Embedding,
}

/// One brute-force nearest-neighbour index per namespaced collection
/// (§22). Held behind a single `RwLock` per collection map -- reasonable
/// for a single-user, local-first app (§2.1) where the dominant operation
/// pattern is "many reads during a search, occasional writes during
/// indexing", not high-concurrency writes.
pub struct EmbeddedVectorStore {
    storage_dir: Option<std::path::PathBuf>,
    collections: RwLock<HashMap<String, Vec<VectorRecord>>>,
}

impl EmbeddedVectorStore {
    /// A pure in-memory store (used by default and by every test in this
    /// crate) -- nothing is written to disk.
    pub fn in_memory() -> Self {
        Self {
            storage_dir: None,
            collections: RwLock::new(HashMap::new()),
        }
    }

    /// A store that additionally persists each collection to
    /// `storage_dir/<collection>.json` on every write (§23: "Storage
    /// locations for AI Cache ... defaults to an app data directory,
    /// user-overridable" -- `storage_dir` is exactly that configured
    /// location, passed in by the caller rather than hardcoded here).
    pub fn with_storage_dir(storage_dir: impl Into<std::path::PathBuf>) -> Self {
        let storage_dir = storage_dir.into();
        let _ = std::fs::create_dir_all(&storage_dir);
        let mut collections = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if let Ok(records) = serde_json::from_slice::<Vec<VectorRecord>>(&bytes) {
                                collections.insert(stem.to_string(), records);
                            }
                        }
                    }
                }
            }
        }
        Self {
            storage_dir: Some(storage_dir),
            collections: RwLock::new(collections),
        }
    }

    fn flush_collection(&self, collection: &str, records: &[VectorRecord]) -> Result<(), AppError> {
        if let Some(dir) = &self.storage_dir {
            let path = dir.join(format!("{collection}.json"));
            let bytes = serde_json::to_vec(records)
                .map_err(|e| AppError::vector_storage(format!("failed to serialize vector collection: {e}")))?;
            std::fs::write(&path, bytes)
                .map_err(|e| AppError::vector_storage(format!("failed to persist vector collection: {e}")))?;
        }
        Ok(())
    }

    pub fn upsert(
        &self,
        collection: &str,
        chunk_id: ChunkId,
        vector: Embedding,
    ) -> Result<String, AppError> {
        let vector_id = format!("{collection}:{}", chunk_id.0);
        let mut collections = self
            .collections
            .write()
            .map_err(|_| AppError::vector_storage("vector store lock poisoned"))?;
        let records = collections.entry(collection.to_string()).or_default();
        if let Some(existing) = records.iter_mut().find(|r| r.chunk_id == chunk_id) {
            existing.vector = vector;
            existing.vector_id = vector_id.clone();
        } else {
            records.push(VectorRecord {
                vector_id: vector_id.clone(),
                chunk_id,
                vector,
            });
        }
        self.flush_collection(collection, records)?;
        Ok(vector_id)
    }

    pub fn delete(&self, collection: &str, chunk_id: ChunkId) -> Result<(), AppError> {
        let mut collections = self
            .collections
            .write()
            .map_err(|_| AppError::vector_storage("vector store lock poisoned"))?;
        if let Some(records) = collections.get_mut(collection) {
            records.retain(|r| r.chunk_id != chunk_id);
            let snapshot = records.clone();
            self.flush_collection(collection, &snapshot)?;
        }
        Ok(())
    }

    pub fn find_by_chunk(
        &self,
        collection: &str,
        chunk_id: ChunkId,
    ) -> Result<Option<(String, Embedding)>, AppError> {
        let collections = self
            .collections
            .read()
            .map_err(|_| AppError::vector_storage("vector store lock poisoned"))?;
        Ok(collections
            .get(collection)
            .and_then(|records| records.iter().find(|r| r.chunk_id == chunk_id))
            .map(|r| (r.vector_id.clone(), r.vector.clone())))
    }

    /// Brute-force cosine-similarity nearest-neighbour search (§18:
    /// "Vector search (Embedding Engine -> Vector DB) over chunk
    /// embeddings"). O(n) over the collection -- adequate for a
    /// single-user local workspace's corpus size (§25 performance goals
    /// target sub-second retrieval overhead, not web-scale ANN search);
    /// swapping in Qdrant/LanceDB's real ANN index behind this same
    /// method signature is the intended future upgrade path.
    pub fn search(
        &self,
        collection: &str,
        query_vector: &Embedding,
        limit: usize,
    ) -> Result<Vec<(ChunkId, f32)>, AppError> {
        let collections = self
            .collections
            .read()
            .map_err(|_| AppError::vector_storage("vector store lock poisoned"))?;
        let Some(records) = collections.get(collection) else {
            return Ok(Vec::new());
        };
        let mut scored: Vec<(ChunkId, f32)> = records
            .iter()
            .map(|r| (r.chunk_id, cosine_similarity(&r.vector, query_vector)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }
}

impl Default for EmbeddedVectorStore {
    fn default() -> Self {
        Self::in_memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_then_find_by_chunk_round_trips() {
        let store = EmbeddedVectorStore::in_memory();
        store
            .upsert("ws-1", ChunkId(1), vec![1.0, 0.0, 0.0])
            .unwrap();
        let found = store.find_by_chunk("ws-1", ChunkId(1)).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn upsert_replaces_existing_vector_for_the_same_chunk() {
        let store = EmbeddedVectorStore::in_memory();
        store.upsert("ws-1", ChunkId(1), vec![1.0, 0.0]).unwrap();
        store.upsert("ws-1", ChunkId(1), vec![0.0, 1.0]).unwrap();
        let (_, vector) = store.find_by_chunk("ws-1", ChunkId(1)).unwrap().unwrap();
        assert_eq!(vector, vec![0.0, 1.0]);
    }

    #[test]
    fn delete_removes_the_vector() {
        let store = EmbeddedVectorStore::in_memory();
        store.upsert("ws-1", ChunkId(1), vec![1.0, 0.0]).unwrap();
        store.delete("ws-1", ChunkId(1)).unwrap();
        assert!(store.find_by_chunk("ws-1", ChunkId(1)).unwrap().is_none());
    }

    #[test]
    fn search_ranks_closest_vector_first() {
        let store = EmbeddedVectorStore::in_memory();
        store.upsert("ws-1", ChunkId(1), vec![1.0, 0.0]).unwrap();
        store.upsert("ws-1", ChunkId(2), vec![0.0, 1.0]).unwrap();
        store.upsert("ws-1", ChunkId(3), vec![0.9, 0.1]).unwrap();

        let results = store.search("ws-1", &vec![1.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, ChunkId(1));
        assert_eq!(results[1].0, ChunkId(3));
    }

    #[test]
    fn search_against_unknown_collection_returns_empty() {
        let store = EmbeddedVectorStore::in_memory();
        assert!(store.search("nope", &vec![1.0], 5).unwrap().is_empty());
    }

    #[test]
    fn collections_are_namespaced_independently() {
        let store = EmbeddedVectorStore::in_memory();
        store.upsert("ws-1", ChunkId(1), vec![1.0, 0.0]).unwrap();
        store.upsert("ws-2", ChunkId(1), vec![0.0, 1.0]).unwrap();

        let (_, v1) = store.find_by_chunk("ws-1", ChunkId(1)).unwrap().unwrap();
        let (_, v2) = store.find_by_chunk("ws-2", ChunkId(1)).unwrap().unwrap();
        assert_ne!(v1, v2);
    }

    #[test]
    fn persisted_store_survives_reconstruction_from_disk() {
        let dir = std::env::temp_dir().join(format!(
            "atlas-vector-store-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        {
            let store = EmbeddedVectorStore::with_storage_dir(&dir);
            store.upsert("ws-1", ChunkId(1), vec![1.0, 0.0]).unwrap();
        }

        let reopened = EmbeddedVectorStore::with_storage_dir(&dir);
        assert!(reopened.find_by_chunk("ws-1", ChunkId(1)).unwrap().is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
