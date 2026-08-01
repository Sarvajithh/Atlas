//! Reranker (§14.1, §18). Runs after the Retriever's hybrid merge, giving
//! every candidate a single query-aware relevance score so ordering
//! doesn't just reflect whichever retrieval path happened to score it
//! higher (§18: "Reranking (cross-encoder or lightweight scoring)"). A
//! full cross-encoder model is Ollama-backed inference, out of scope for
//! this milestone (§28); the lightweight scoring alternative the
//! architecture doc explicitly allows is implemented here instead --
//! term-overlap plus a small bonus for exact phrase containment.

use atlas_types::retrieval::SearchHit;

pub struct Reranker;

impl Reranker {
    pub fn new() -> Self {
        Self
    }

    /// Re-score and re-sort `hits` against `query` (§18). Reranking is
    /// independent of how a hit was originally retrieved -- it only looks
    /// at `query` and `hit.text_content`.
    pub fn rerank(&self, query: &str, mut hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let query_terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        let query_lower = query.to_lowercase();

        for hit in hits.iter_mut() {
            hit.score = Self::relevance_score(&query_terms, &query_lower, &hit.text_content);
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits
    }

    fn relevance_score(query_terms: &[String], query_lower: &str, text: &str) -> f32 {
        if query_terms.is_empty() {
            return 0.0;
        }
        let text_lower = text.to_lowercase();
        let overlap = query_terms
            .iter()
            .filter(|t| text_lower.contains(t.as_str()))
            .count() as f32
            / query_terms.len() as f32;

        // Exact phrase containment is a strong relevance signal a plain
        // per-term overlap score misses -- a lightweight stand-in for what
        // a cross-encoder would otherwise capture (§18).
        let phrase_bonus = if !query_lower.trim().is_empty() && text_lower.contains(query_lower.trim()) {
            0.25
        } else {
            0.0
        };

        (overlap + phrase_bonus).min(1.0)
    }
}

impl Default for Reranker {
    fn default() -> Self {
        Self::new()
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
            page_or_location_ref: "1".to_string(),
            score: 0.0,
        }
    }

    #[test]
    fn rerank_orders_by_term_overlap_with_the_query() {
        let reranker = Reranker::new();
        let hits = vec![
            hit(1, "bananas and potassium content"),
            hit(2, "gradient descent minimizes the loss function"),
        ];
        let reranked = reranker.rerank("gradient descent loss", hits);
        assert_eq!(reranked[0].chunk_id, ChunkId(2));
    }

    #[test]
    fn exact_phrase_match_scores_higher_than_scattered_terms() {
        let reranker = Reranker::new();
        let hits = vec![
            hit(1, "loss function and gradient are both mentioned separately"),
            hit(2, "gradient descent loss appears as an exact phrase"),
        ];
        let reranked = reranker.rerank("gradient descent loss", hits);
        assert_eq!(reranked[0].chunk_id, ChunkId(2));
    }

    #[test]
    fn empty_query_gives_every_hit_a_zero_score() {
        let reranker = Reranker::new();
        let hits = vec![hit(1, "anything"), hit(2, "something else")];
        let reranked = reranker.rerank("   ", hits);
        assert!(reranked.iter().all(|h| h.score == 0.0));
    }
}
