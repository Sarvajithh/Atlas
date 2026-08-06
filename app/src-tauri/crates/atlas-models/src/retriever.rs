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

/// Part 4 (retrieval-behavior audit): the Retriever's `limit` parameter is
/// the *final* number of chunks a caller ultimately wants, but truncating
/// to exactly that count immediately after the hybrid merge -- before
/// reranking ever runs -- means the reranker can only ever reorder within
/// whatever the raw weighted-merge score already chose, and can never
/// promote a genuinely more relevant chunk that merge happened to score
/// just outside the cutoff. Retrieval now fetches and merges a wider
/// candidate pool (`limit * CANDIDATE_POOL_MULTIPLIER`) and returns that
/// whole pool; `ContextBuilder::assemble` (§39.1) is what actually narrows
/// it down, via reranking against the real query followed by dynamic
/// token budgeting -- matching the intended pipeline order (hybrid
/// retrieval -> merge -> dedupe -> rerank -> dynamic token budgeting)
/// rather than a naive top-k cut before reranking ever sees the rest.
const CANDIDATE_POOL_MULTIPLIER: usize = 3;

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
    ///
    /// Part 4 (retrieval-behavior audit): `limit` describes how many
    /// chunks the caller ultimately wants, but this method returns a wider
    /// candidate pool (`limit * CANDIDATE_POOL_MULTIPLIER`, see its doc)
    /// rather than truncating to exactly `limit` here -- the caller's
    /// `ContextBuilder::assemble` reranks and token-budgets that pool down
    /// to the real final set, so a chunk that merge scored just outside a
    /// naive top-`limit` cut still gets a fair chance to be promoted by
    /// the reranker instead of being discarded before it ever runs.
    pub fn retrieve(
        &self,
        workspace_id: WorkspaceId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let __t0 = std::time::Instant::now();
        atlas_utils::log_info!("[Retriever] entered workspace_id={} query={query:?} limit={limit}", workspace_id.0);

        let candidate_pool = limit.saturating_mul(CANDIDATE_POOL_MULTIPLIER).max(limit);
        let keyword_hits = self.keyword_search.search(workspace_id, query, candidate_pool)?;
        atlas_utils::log_info!("[Retriever] keyword_search returned {} hits", keyword_hits.len());

        let query_vector = self.embedder.embed(query)?;
        let raw_vector_hits = self
            .vector_search
            .search(workspace_id, &query_vector, candidate_pool)?;
        atlas_utils::log_info!("[Retriever] vector_search returned {} raw hits", raw_vector_hits.len());
        let vector_hits = self.hydrate_vector_hits(raw_vector_hits)?;
        atlas_utils::log_info!("[Retriever] vector hits hydrated to {} (chunk lookups that missed are dropped)", vector_hits.len());

        let merged = self.merge(keyword_hits, vector_hits, candidate_pool);
        atlas_utils::log_info!(
            "[Retriever] exited, returned {} merged candidates (pool={candidate_pool}, final narrowing happens in ContextBuilder) elapsed={:?}",
            merged.len(),
            __t0.elapsed()
        );
        Ok(merged)
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
    fn retrieve_returns_a_wider_candidate_pool_than_the_final_limit() {
        // Part 4: Retriever no longer truncates to exactly `limit` -- it
        // returns a wider pool (limit * CANDIDATE_POOL_MULTIPLIER) so
        // ContextBuilder's reranker gets a real chance to reorder/promote
        // candidates instead of only ever seeing an already-cut top-k.
        let retriever = Retriever::new(
            Arc::new(FixedKeywordSearch(vec![hit(1, 1.0), hit(2, 1.0), hit(3, 1.0)])),
            Arc::new(FixedVectorSearch(vec![])),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(FixedChunks(vec![chunk(1, "a"), chunk(2, "b"), chunk(3, "c")])),
        );
        let results = retriever.retrieve(WorkspaceId(1), "x", 2).unwrap();
        // All 3 available hits survive into the candidate pool (2 * 3 = 6
        // >= 3), rather than being cut down to 2 before reranking runs.
        assert_eq!(results.len(), 3);
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
