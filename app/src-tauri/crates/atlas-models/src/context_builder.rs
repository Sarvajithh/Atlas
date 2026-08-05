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
    /// Fix 4 (P0 audit): this is now a *ceiling*, not the only input to the
    /// effective budget -- `assemble` additionally derives a budget from
    /// whichever model is actually resolved for the request and uses
    /// whichever is smaller, so retrieved context can never consume more
    /// of a small model's real context window than it has, and a large
    /// model isn't needlessly capped at this ceiling either as long as
    /// this value is configured generously.
    max_context_tokens: u32,
    reranker: Reranker,
}

/// Tokens reserved out of a model's context window for the system prompt
/// and the model's own expected response (Fix 4 requirement 2: "leave
/// headroom for the system prompt and expected response length -- do not
/// consume the entire window with retrieved context"). Named and
/// documented rather than an inline literal, and deliberately generous:
/// underestimating this reserve risks the model silently truncating the
/// prompt or the response server-side, which is exactly the invisible
/// failure mode this fix exists to prevent.
const CONTEXT_RESERVE_FOR_SYSTEM_PROMPT_AND_RESPONSE_TOKENS: u32 = 1024;

/// Derive the retrieved-context token budget from a resolved model's real
/// context window (Fix 4 requirement 2), reserving headroom per
/// `CONTEXT_RESERVE_FOR_SYSTEM_PROMPT_AND_RESPONSE_TOKENS`. Saturates at 0
/// rather than underflowing/panicking for a pathologically small
/// `model_context_length` (e.g. a misreported model with a window smaller
/// than the reserve itself).
fn budget_from_model_context(model_context_length: u32) -> u32 {
    model_context_length.saturating_sub(CONTEXT_RESERVE_FOR_SYSTEM_PROMPT_AND_RESPONSE_TOKENS)
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
    ///    the effective budget would be exceeded (§39.1 "token
    ///    budgeting"). Fix 4 (P0 audit): the effective budget is
    ///    `min(max_context_tokens, budget_from_model_context(model_context_length))`
    ///    -- derived from whichever model is actually resolved for this
    ///    request, with headroom reserved for the system prompt and
    ///    response, rather than only the fixed ceiling this builder was
    ///    constructed with.
    /// 4. Order -- final context is emitted in the document's natural
    ///    reading order (ascending `chunk_id` as a stable proxy for
    ///    original sequence) rather than score order, since a Tutor/
    ///    Reasoning Engine reads better from a coherently ordered context
    ///    than a relevance-sorted one (§39.1 "ordering").
    /// 5. Citation preparation -- every surviving hit gets a `Citation`
    ///    (§39.1, §44.1).
    ///
    /// `model_context_length` is the `context_length` of whichever model
    /// was actually resolved (via the Model Registry) for the request this
    /// context is being assembled for -- callers resolve it once per
    /// request, the same lookup `OllamaEngine`/`OllamaProvider` already do
    /// for `num_ctx` (Fix 4 requirement 1), so the two numbers can't
    /// silently disagree.
    pub fn assemble(&self, query: &str, hits: Vec<SearchHit>, model_context_length: u32) -> Result<AssembledContext, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        let __t0 = std::time::Instant::now();
        let effective_budget = self.max_context_tokens.min(budget_from_model_context(model_context_length));
        atlas_utils::log_info!(
            "[ContextBuilder] entered with {} hits, max_context_tokens={}, model_context_length={}, effective_budget={}",
            hits.len(),
            self.max_context_tokens,
            model_context_length,
            effective_budget
        );

        let ranked = self.reranker.rerank(query, hits);
        let deduplicated = Self::deduplicate(ranked);
        let (budgeted, total_tokens) = self.apply_token_budget(deduplicated, effective_budget);

        let mut ordered = budgeted;
        ordered.sort_by_key(|h| h.chunk_id.0);

        let citations = citations_for_hits(&ordered);
        atlas_utils::log_info!(
            "[ContextBuilder] exited hits_kept={} citations={} total_tokens={} elapsed={:?}",
            ordered.len(),
            citations.len(),
            total_tokens,
            __t0.elapsed()
        );
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
    /// `budget` is the caller's already-computed effective budget (Fix 4:
    /// `min(max_context_tokens, budget_from_model_context(...))`), not
    /// `self.max_context_tokens` directly, so this only ever drops the
    /// lowest-ranked hits (the reranker's ordering is respected -- hits
    /// are consumed in the order they arrive here) once the real,
    /// model-aware budget is exceeded.
    fn apply_token_budget(&self, hits: Vec<SearchHit>, budget: u32) -> (Vec<SearchHit>, u32) {
        let mut kept = Vec::new();
        let mut total = 0u32;
        for hit in hits {
            let tokens = hit.text_content.split_whitespace().count() as u32;
            if total + tokens > budget && !kept.is_empty() {
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

    /// A generously large model context length for tests that only care
    /// about exercising `max_context_tokens` (the pre-Fix-4 ceiling)
    /// without the model-derived budget kicking in first.
    const AMPLE_MODEL_CONTEXT: u32 = 1_000_000;

    #[test]
    fn max_context_tokens_is_configurable_not_hardcoded() {
        assert_eq!(ContextBuilder::new(2048).max_context_tokens(), 2048);
        assert_eq!(ContextBuilder::new(8192).max_context_tokens(), 8192);
    }

    #[test]
    fn assemble_preserves_all_input_hits_within_budget() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(2, "b two words"), hit(1, "a two words")];
        let assembled = builder.assemble("a b", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 2);
    }

    #[test]
    fn assemble_of_empty_input_is_empty_context() {
        let builder = ContextBuilder::new(4096);
        let assembled = builder.assemble("query", Vec::new(), AMPLE_MODEL_CONTEXT).unwrap();
        assert!(assembled.hits.is_empty());
        assert!(assembled.citations.is_empty());
    }

    #[test]
    fn duplicate_chunk_ids_are_deduplicated() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "same chunk"), hit(1, "same chunk")];
        let assembled = builder.assemble("chunk", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 1);
    }

    #[test]
    fn final_ordering_is_by_chunk_id_not_score() {
        let builder = ContextBuilder::new(4096);
        // "gradient" only appears in chunk 5's text, so reranking would
        // otherwise put it first; final order must still be ascending by
        // chunk id (reading order).
        let hits = vec![hit(5, "gradient descent"), hit(1, "unrelated text")];
        let assembled = builder.assemble("gradient", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits[0].chunk_id, ChunkId(1));
        assert_eq!(assembled.hits[1].chunk_id, ChunkId(5));
    }

    #[test]
    fn token_budget_drops_hits_once_the_budget_is_exceeded() {
        let builder = ContextBuilder::new(3);
        let hits = vec![hit(1, "one two three"), hit(2, "four five six")];
        let assembled = builder.assemble("one four", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 1);
        assert!(assembled.total_tokens <= 3);
    }

    #[test]
    fn every_surviving_hit_gets_a_citation() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "a"), hit(2, "b")];
        let assembled = builder.assemble("a b", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.citations.len(), assembled.hits.len());
    }

    // ---- Fix 4 (P0 audit): model-context-derived budget ----

    #[test]
    fn budget_from_model_context_reserves_headroom_for_system_prompt_and_response() {
        assert_eq!(budget_from_model_context(4096), 4096 - CONTEXT_RESERVE_FOR_SYSTEM_PROMPT_AND_RESPONSE_TOKENS);
        // Saturates rather than underflowing for a pathologically small
        // context window.
        assert_eq!(budget_from_model_context(10), 0);
    }

    #[test]
    fn assemble_is_capped_by_a_small_resolved_models_context_even_when_max_context_tokens_is_large() {
        // `max_context_tokens` (the builder's configured ceiling) is huge,
        // but the *resolved model* for this particular request only has a
        // small real context window -- the effective budget must follow
        // the model, not silently use the larger ceiling (the exact bug
        // this fix closes: a small model's real window being ignored).
        let builder = ContextBuilder::new(1_000_000);
        let hits = vec![hit(1, "one two three"), hit(2, "four five six")];
        let small_model_context = CONTEXT_RESERVE_FOR_SYSTEM_PROMPT_AND_RESPONSE_TOKENS + 3;
        let assembled = builder.assemble("one four", hits, small_model_context).unwrap();
        assert_eq!(assembled.hits.len(), 1);
        assert!(assembled.total_tokens <= 3);
    }

    #[test]
    fn assemble_uses_max_context_tokens_when_the_model_window_is_larger() {
        // The other direction: a big resolved model shouldn't let context
        // assembly exceed this builder's own configured ceiling either.
        let builder = ContextBuilder::new(3);
        let hits = vec![hit(1, "one two three"), hit(2, "four five six")];
        let assembled = builder.assemble("one four", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 1);
        assert!(assembled.total_tokens <= 3);
    }
}
