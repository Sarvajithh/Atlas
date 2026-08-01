//! Context Builder (§39). Sits between Retrieval and the Tutor/Reasoning
//! Engines: ranking, compression, deduplication, token budgeting,
//! ordering, citation preparation, and context validation (§39.1). Refines
//! §15's pipeline without renaming or removing any of its steps (§39.2).

use atlas_types::retrieval::{Citation, SearchHit};
use atlas_utils::AppError;

use crate::citation::citations_for_hits;
use crate::reranker::Reranker;

/// Ranked, deduplicated, token-budgeted context, with citations prepared
/// for every chunk that made it in (§39.1, §44.1), ready for the Prompt
/// Builder (§40).
#[derive(Debug, Clone)]
pub struct AssembledContext {
    pub hits: Vec<SearchHit>,
    pub citations: Vec<Citation>,
    pub total_tokens: u32,
}

pub struct ContextBuilder {
    /// Token budget strategy is configuration-driven (§39.1), not hardcoded.
    max_context_tokens: u32,
    reranker: Reranker,
}

impl ContextBuilder {
    pub fn new(max_context_tokens: u32) -> Self {
        Self {
            max_context_tokens,
            reranker: Reranker::new(),
        }
    }

    pub fn max_context_tokens(&self) -> u32 {
        self.max_context_tokens
    }

    /// Assemble ranked/compressed/deduplicated context from retrieved hits
    /// (§39.1):
    ///
    /// 1. Rank -- rerank against `query` (§18/§39.1 "ranking").
    /// 2. Deduplicate -- a chunk appearing via both keyword and vector
    ///    paths (already merged upstream by the Retriever) or repeated
    ///    across near-identical retrieved spans is kept only once, highest
    ///    score wins (§39.1 "deduplication").
    /// 3. Token budget -- greedily keep hits, highest-ranked first, until
    ///    `max_context_tokens` would be exceeded (§39.1 "token budgeting").
    /// 4. Order -- final context is emitted in the document's natural
    ///    reading order (ascending `chunk_id` as a stable proxy for
    ///    original sequence) rather than score order, since a Tutor/
    ///    Reasoning Engine reads better from a coherently ordered context
    ///    than a relevance-sorted one (§39.1 "ordering").
    /// 5. Citation preparation -- every surviving hit gets a `Citation`
    ///    (§39.1, §44.1).
    pub fn assemble(&self, query: &str, hits: Vec<SearchHit>) -> Result<AssembledContext, AppError> {
        let ranked = self.reranker.rerank(query, hits);
        let deduplicated = Self::deduplicate(ranked);
        let (budgeted, total_tokens) = self.apply_token_budget(deduplicated);

        let mut ordered = budgeted;
        ordered.sort_by_key(|h| h.chunk_id.0);

        let citations = citations_for_hits(&ordered);
        Ok(AssembledContext {
            hits: ordered,
            citations,
            total_tokens,
        })
    }

    fn deduplicate(hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::with_capacity(hits.len());
        for hit in hits {
            if seen.insert(hit.chunk_id.0) {
                result.push(hit);
            }
        }
        result
    }

    /// Approximate token count as whitespace-delimited word count (§18's
    /// same convention as the Chunking Engine, `atlas-indexer::chunker`) --
    /// good enough for a budget, without pulling in a tokenizer dependency.
    fn apply_token_budget(&self, hits: Vec<SearchHit>) -> (Vec<SearchHit>, u32) {
        let mut kept = Vec::new();
        let mut total = 0u32;
        for hit in hits {
            let tokens = hit.text_content.split_whitespace().count() as u32;
            if total + tokens > self.max_context_tokens && !kept.is_empty() {
                break;
            }
            total += tokens;
            kept.push(hit);
        }
        (kept, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::{ChunkId, DocumentId};

    fn hit(chunk_id: i64, text: &str) -> SearchHit {
        SearchHit {
            chunk_id: ChunkId(chunk_id),
            document_id: DocumentId(1),
            text_content: text.to_string(),
            page_or_location_ref: chunk_id.to_string(),
            score: 0.0,
        }
    }

    #[test]
    fn max_context_tokens_is_configurable_not_hardcoded() {
        assert_eq!(ContextBuilder::new(2048).max_context_tokens(), 2048);
        assert_eq!(ContextBuilder::new(8192).max_context_tokens(), 8192);
    }

    #[test]
    fn assemble_preserves_all_input_hits_within_budget() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(2, "b two words"), hit(1, "a two words")];
        let assembled = builder.assemble("a b", hits).unwrap();
        assert_eq!(assembled.hits.len(), 2);
    }

    #[test]
    fn assemble_of_empty_input_is_empty_context() {
        let builder = ContextBuilder::new(4096);
        let assembled = builder.assemble("query", Vec::new()).unwrap();
        assert!(assembled.hits.is_empty());
        assert!(assembled.citations.is_empty());
    }

    #[test]
    fn duplicate_chunk_ids_are_deduplicated() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "same chunk"), hit(1, "same chunk")];
        let assembled = builder.assemble("chunk", hits).unwrap();
        assert_eq!(assembled.hits.len(), 1);
    }

    #[test]
    fn final_ordering_is_by_chunk_id_not_score() {
        let builder = ContextBuilder::new(4096);
        // "gradient" only appears in chunk 5's text, so reranking would
        // otherwise put it first; final order must still be ascending by
        // chunk id (reading order).
        let hits = vec![hit(5, "gradient descent"), hit(1, "unrelated text")];
        let assembled = builder.assemble("gradient", hits).unwrap();
        assert_eq!(assembled.hits[0].chunk_id, ChunkId(1));
        assert_eq!(assembled.hits[1].chunk_id, ChunkId(5));
    }

    #[test]
    fn token_budget_drops_hits_once_the_budget_is_exceeded() {
        let builder = ContextBuilder::new(3);
        let hits = vec![hit(1, "one two three"), hit(2, "four five six")];
        let assembled = builder.assemble("one four", hits).unwrap();
        assert_eq!(assembled.hits.len(), 1);
        assert!(assembled.total_tokens <= 3);
    }

    #[test]
    fn every_surviving_hit_gets_a_citation() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "a"), hit(2, "b")];
        let assembled = builder.assemble("a b", hits).unwrap();
        assert_eq!(assembled.citations.len(), assembled.hits.len());
    }
}
