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
        // Part 3 (context-quality audit): remove near-identical fragments
        // before budgeting -- most commonly duplicate/near-duplicate OCR
        // output for the same source region (e.g. a scanned page OCR'd
        // more than once, or two overlapping chunk windows that captured
        // almost the same handwritten line). These aren't caught by
        // `deduplicate`'s exact chunk_id check since they're genuinely
        // different chunk_ids with near-identical *text* -- left in, they
        // waste token budget on redundant content and dilute the model's
        // attention with repeated material instead of more distinct
        // context.
        let condensed = Self::remove_near_duplicates(deduplicated);
        let (budgeted, total_tokens) = self.apply_token_budget(condensed, effective_budget);

        let mut ordered = budgeted;
        ordered.sort_by_key(|h| h.chunk_id.0);
        // Part 3: merge adjacent chunks from the same document (consecutive
        // chunk_ids) into a single combined block once final reading-order
        // is established -- a Tutor/Reasoning Engine reads one coherent
        // passage far better than several artificially-split fragments of
        // the same paragraph, and it collapses citation noise (one [n] for
        // one contiguous passage instead of three for what was originally
        // one). Ordering is preserved; only truly consecutive same-document
        // chunks are combined, so unrelated hits are never merged together.
        let ordered = Self::merge_adjacent(ordered);

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

    /// Normalize text for near-duplicate comparison: lowercase, collapse
    /// all whitespace runs to a single space, trim. Deliberately simple
    /// (no stemming/fuzzy matching dependency) -- this only needs to catch
    /// the common case of the same underlying text re-OCR'd or re-chunked
    /// with different incidental whitespace/casing, not genuinely
    /// paraphrased duplicates.
    fn normalize_for_dedup(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
    }

    /// Threshold above which two chunks' normalized text is treated as a
    /// near-duplicate rather than genuinely distinct content (Part 3,
    /// "remove near-identical OCR fragments"). Named/documented rather
    /// than inlined; deliberately conservative (high) so genuinely
    /// distinct-but-similar passages (e.g. two problems from the same
    /// worked example) are never dropped -- only content that is almost
    /// entirely the same is treated as redundant.
    const NEAR_DUPLICATE_OVERLAP_THRESHOLD: f32 = 0.9;

    /// Remove hits whose normalized text is a near-duplicate of an
    /// already-kept hit (Part 3). Input order is the reranker's ranking
    /// order (best first), so ties are resolved in favor of whichever hit
    /// was already kept -- the higher-ranked one always wins, never an
    /// arbitrary later duplicate replacing it. Containment (one chunk's
    /// text is almost entirely inside another's) and near-equal length
    /// overlap both count, since duplicate OCR passes commonly produce
    /// slightly different chunk boundaries around the same underlying
    /// text rather than byte-identical strings.
    fn remove_near_duplicates(hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let mut kept: Vec<SearchHit> = Vec::with_capacity(hits.len());
        let mut kept_normalized: Vec<String> = Vec::with_capacity(hits.len());

        for hit in hits {
            let normalized = Self::normalize_for_dedup(&hit.text_content);
            let is_near_duplicate = kept_normalized.iter().any(|existing| {
                if normalized.is_empty() || existing.is_empty() {
                    return false;
                }
                if existing.contains(&normalized) || normalized.contains(existing) {
                    let shorter = normalized.len().min(existing.len()) as f32;
                    let longer = normalized.len().max(existing.len()) as f32;
                    return shorter / longer >= Self::NEAR_DUPLICATE_OVERLAP_THRESHOLD;
                }
                false
            });
            if !is_near_duplicate {
                kept_normalized.push(normalized);
                kept.push(hit);
            }
        }
        kept
    }

    /// Merge consecutive same-document chunks (chunk_id differing by
    /// exactly 1) into a single combined hit (Part 3, "merge adjacent
    /// chunks"). `hits` must already be sorted into final reading order
    /// (ascending chunk_id) -- this only ever combines genuinely adjacent
    /// material, never chunks from different documents or with a gap
    /// between them, so unrelated passages are never stitched together.
    /// The merged hit keeps the first chunk's `chunk_id`/location
    /// reference (so its citation still resolves to the start of the
    /// passage) and the highest of the merged chunks' scores.
    fn merge_adjacent(hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let mut merged: Vec<SearchHit> = Vec::with_capacity(hits.len());
        // Tracks the *original* chunk_id most recently folded into
        // `merged.last()`, kept separate from the hit's own (unchanged)
        // `chunk_id` field -- the exposed hit keeps citing the first
        // chunk in the run (so its citation points at the start of the
        // passage), while adjacency is still tested against the true
        // last-seen original id so a third, fourth, ... consecutive chunk
        // keeps chaining onto the same merged hit correctly.
        let mut last_original_id: Option<i64> = None;
        for hit in hits {
            let is_adjacent = merged
                .last()
                .zip(last_original_id)
                .is_some_and(|(last, last_id)| last.document_id == hit.document_id && hit.chunk_id.0 == last_id + 1);
            if is_adjacent {
                let last = merged.last_mut().expect("checked by is_adjacent");
                last.text_content.push('\n');
                last.text_content.push_str(&hit.text_content);
                last.score = last.score.max(hit.score);
                last_original_id = Some(hit.chunk_id.0);
            } else {
                last_original_id = Some(hit.chunk_id.0);
                merged.push(hit);
            }
        }
        merged
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
    fn assemble_preserves_all_input_hits_within_budget_when_not_adjacent() {
        // chunk_id 1 and 3 (a gap, not consecutive) so Part 3's adjacent-
        // chunk merge doesn't fold them together -- isolates "within
        // budget" from the separate merge behavior (see
        // `assemble_merges_adjacent_same_document_chunks` below).
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(3, "b two words"), hit(1, "a two words")];
        let assembled = builder.assemble("a b", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 2);
    }

    // ---- Part 3 (context-quality audit): merge + near-duplicate removal ----

    #[test]
    fn assemble_merges_adjacent_same_document_chunks() {
        // chunk_id 1 and 2, same document -- genuinely consecutive
        // material that reads better as one combined passage than two
        // artificially split fragments.
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "the first half of a sentence"), hit(2, "continues into the second half")];
        let assembled = builder.assemble("sentence", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 1);
        assert!(assembled.hits[0].text_content.contains("first half"));
        assert!(assembled.hits[0].text_content.contains("second half"));
    }

    #[test]
    fn assemble_does_not_merge_non_adjacent_chunks() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![hit(1, "unrelated passage one"), hit(9, "unrelated passage two")];
        let assembled = builder.assemble("passage", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 2);
    }

    #[test]
    fn assemble_removes_near_duplicate_ocr_fragments() {
        // Same underlying text, re-OCR'd/re-chunked with trivial
        // whitespace/casing differences and non-adjacent chunk_ids (so
        // this exercises near-duplicate removal specifically, not the
        // adjacent-chunk merge above).
        let builder = ContextBuilder::new(4096);
        let hits = vec![
            hit(1, "Differential privacy adds calibrated noise to query results."),
            hit(50, "differential  privacy adds calibrated noise to query results"),
        ];
        let assembled = builder.assemble("differential privacy", hits, AMPLE_MODEL_CONTEXT).unwrap();
        assert_eq!(assembled.hits.len(), 1);
    }

    #[test]
    fn assemble_keeps_genuinely_distinct_chunks_even_if_similar_topic() {
        let builder = ContextBuilder::new(4096);
        let hits = vec![
            hit(1, "Differential privacy uses a privacy budget called epsilon."),
            hit(50, "K-anonymity is a different, older approach to anonymization."),
        ];
        let assembled = builder.assemble("privacy", hits, AMPLE_MODEL_CONTEXT).unwrap();
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
