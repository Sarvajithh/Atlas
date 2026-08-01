//! Retriever (§14.1, §18 "Hybrid retrieval"). Combines keyword search
//! (`KeywordSearchRepository`, owned by atlas-indexer, implemented by
//! atlas-db) and vector search (`VectorSearchRepository`, owned by
//! atlas-indexer, implemented by atlas-vector) into one merged candidate
//! list, embedding the query itself via the same `EmbeddingEngine` used at
//! index time so query and chunk vectors live in the same space.
//!
//! ```text
//! query
//!   +--> KeywordSearchRepository.search  --> SearchHit[] (lexical)
//!   +--> EmbeddingEngine.embed(query)
//!         --> VectorSearchRepository.search --> SearchHit[] (semantic)
//!               \
//!                +--> merge (§18: weighted score combination, de-duped by chunk_id)
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use atlas_indexer::embedding::EmbeddingEngine;
use atlas_indexer::keyword_search::KeywordSearchRepository;
use atlas_indexer::vector_search::VectorSearchRepository;
use atlas_indexer::ChunkRepository;
use atlas_types::ids::WorkspaceId;
use atlas_types::retrieval::SearchHit;
use atlas_utils::AppError;

/// Relative weighting between the two retrieval signals when merging
/// (§18: "weighted combination of the two", configuration rather than a
/// hardcoded 50/50 split).
#[derive(Debug, Clone, Copy)]
pub struct HybridWeights {
    pub keyword_weight: f32,
    pub vector_weight: f32,
}

impl Default for HybridWeights {
    fn default() -> Self {
        Self {
            keyword_weight: 0.4,
            vector_weight: 0.6,
        }
    }
}

pub struct Retriever {
    keyword_search: Arc<dyn KeywordSearchRepository>,
    vector_search: Arc<dyn VectorSearchRepository>,
    embedder: Arc<dyn EmbeddingEngine>,
    chunks: Arc<dyn ChunkRepository>,
    weights: HybridWeights,
}

impl Retriever {
    pub fn new(
        keyword_search: Arc<dyn KeywordSearchRepository>,
        vector_search: Arc<dyn VectorSearchRepository>,
        embedder: Arc<dyn EmbeddingEngine>,
        chunks: Arc<dyn ChunkRepository>,
    ) -> Self {
        Self {
            keyword_search,
            vector_search,
            embedder,
            chunks,
            weights: HybridWeights::default(),
        }
    }

    pub fn with_weights(mut self, weights: HybridWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Run both retrieval paths and merge their results (§18). Vector hits
    /// only carry a `chunk_id`/score (the vector store doesn't know chunk
    /// text/location, §18) -- this method fills in the missing fields from
    /// `ChunkRepository` before merging, so every returned `SearchHit` is
    /// complete regardless of which path produced it.
    pub fn retrieve(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        let keyword_hits = self.keyword_search.search(workspace_id, query, limit * 2)?;

        let query_vector = self.embedder.embed(query)?;
        let raw_vector_hits = self
            .vector_search
            .search(workspace_id, &query_vector, limit * 2)?;
        let vector_hits = self.hydrate_vector_hits(raw_vector_hits)?;

        Ok(self.merge(keyword_hits, vector_hits, limit))
    }

    /// Fill in `document_id`/`text_content`/`page_or_location_ref` on
    /// vector hits, which only carry a `chunk_id` and score at the
    /// vector-store layer.
    fn hydrate_vector_hits(&self, hits: Vec<SearchHit>) -> Result<Vec<SearchHit>, AppError> {
        let mut hydrated = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(chunk) = self.chunks.find_by_id(hit.chunk_id)? {
                hydrated.push(SearchHit {
                    chunk_id: hit.chunk_id,
                    document_id: chunk.document_id,
                    text_content: chunk.text_content,
                    page_or_location_ref: chunk.page_or_location_ref,
                    score: hit.score,
                });
            }
        }
        Ok(hydrated)
    }

    fn merge(&self, keyword_hits: Vec<SearchHit>, vector_hits: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
        let mut merged: HashMap<i64, SearchHit> = HashMap::new();

        for hit in keyword_hits {
            let entry = merged.entry(hit.chunk_id.0).or_insert_with(|| SearchHit {
                score: 0.0,
                ..hit.clone()
            });
            entry.score += hit.score * self.weights.keyword_weight;
        }
        for hit in vector_hits {
            let entry = merged.entry(hit.chunk_id.0).or_insert_with(|| SearchHit {
                score: 0.0,
                ..hit.clone()
            });
            entry.score += hit.score * self.weights.vector_weight;
        }

        let mut results: Vec<SearchHit> = merged.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_indexer::embedding::HashEmbeddingEngine;
    use atlas_types::chunk::Chunk;
    use atlas_types::ids::{ChunkId, DocumentId};
    use atlas_utils::AppError;

    struct FixedKeywordSearch(Vec<SearchHit>);
    impl KeywordSearchRepository for FixedKeywordSearch {
        fn search(&self, _workspace_id: WorkspaceId, _query: &str, _limit: usize) -> Result<Vec<SearchHit>, AppError> {
            Ok(self.0.clone())
        }
    }

    struct FixedVectorSearch(Vec<SearchHit>);
    impl VectorSearchRepository for FixedVectorSearch {
        fn search(
            &self,
            _workspace_id: WorkspaceId,
            _query_vector: &atlas_indexer::embedding::Embedding,
            _limit: usize,
        ) -> Result<Vec<SearchHit>, AppError> {
            Ok(self.0.clone())
        }
    }

    struct FixedChunks(Vec<Chunk>);
    impl ChunkRepository for FixedChunks {
        fn list_for_document(&self, _document_id: DocumentId) -> Result<Vec<Chunk>, AppError> {
            Ok(self.0.clone())
        }
        fn insert(&self, chunk: Chunk) -> Result<Chunk, AppError> {
            Ok(chunk)
        }
        fn delete_for_document(&self, _document_id: DocumentId) -> Result<(), AppError> {
            Ok(())
        }
        fn find_by_id(&self, id: ChunkId) -> Result<Option<Chunk>, AppError> {
            Ok(self.0.iter().find(|c| c.id == id).cloned())
        }
    }

    fn hit(chunk_id: i64, score: f32) -> SearchHit {
        SearchHit {
            chunk_id: ChunkId(chunk_id),
            document_id: DocumentId(1),
            text_content: format!("chunk {chunk_id}"),
            page_or_location_ref: "1".to_string(),
            score,
        }
    }

    fn chunk(id: i64, text: &str) -> Chunk {
        Chunk {
            id: ChunkId(id),
            document_id: DocumentId(1),
            sequence_index: 0,
            text_content: text.to_string(),
            page_or_location_ref: "1".to_string(),
            token_count: 2,
            parser_version: "v1".to_string(),
        }
    }

    #[test]
    fn retrieve_merges_and_ranks_by_combined_weighted_score() {
        let retriever = Retriever::new(
            Arc::new(FixedKeywordSearch(vec![hit(1, 1.0), hit(2, 0.2)])),
            Arc::new(FixedVectorSearch(vec![hit(2, 1.0)])),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(FixedChunks(vec![chunk(1, "one"), chunk(2, "two")])),
        );

        let results = retriever.retrieve(WorkspaceId(1), "anything", 5).unwrap();
        // chunk 2: 0.2*0.4 (keyword) + 1.0*0.6 (vector) = 0.68
        // chunk 1: 1.0*0.4 (keyword only) = 0.4
        assert_eq!(results[0].chunk_id, ChunkId(2));
        assert_eq!(results[1].chunk_id, ChunkId(1));
    }

    #[test]
    fn retrieve_respects_the_limit() {
        let retriever = Retriever::new(
            Arc::new(FixedKeywordSearch(vec![hit(1, 1.0), hit(2, 1.0), hit(3, 1.0)])),
            Arc::new(FixedVectorSearch(vec![])),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(FixedChunks(vec![chunk(1, "a"), chunk(2, "b"), chunk(3, "c")])),
        );
        let results = retriever.retrieve(WorkspaceId(1), "x", 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn custom_weights_change_the_ranking() {
        let retriever = Retriever::new(
            Arc::new(FixedKeywordSearch(vec![hit(1, 1.0)])),
            Arc::new(FixedVectorSearch(vec![hit(2, 1.0)])),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(FixedChunks(vec![chunk(1, "a"), chunk(2, "b")])),
        )
        .with_weights(HybridWeights {
            keyword_weight: 1.0,
            vector_weight: 0.0,
        });

        let results = retriever.retrieve(WorkspaceId(1), "x", 5).unwrap();
        assert_eq!(results[0].chunk_id, ChunkId(1));
    }
}
