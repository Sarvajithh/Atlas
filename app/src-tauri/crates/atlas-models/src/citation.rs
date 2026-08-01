//! Citation preparation (§39.1: "citation preparation" is one of the
//! Context Builder's named responsibilities; §44.1's Shared Location
//! Reference is what a citation ultimately points at). Kept as its own
//! small module rather than inlined into `context_builder.rs` so it's
//! independently testable and reusable by anything that needs a citation
//! from a chunk (not just the context assembly path).

use atlas_types::retrieval::{Citation, SearchHit};

/// Build a citation for one retrieved chunk (§44.1). `snippet` is
/// deliberately short (not the full chunk text) since a citation is a
/// pointer + preview, not a copy of the source.
pub fn citation_for_hit(hit: &SearchHit) -> Citation {
    Citation {
        document_id: hit.document_id,
        chunk_id: hit.chunk_id,
        location_ref: hit.page_or_location_ref.clone(),
        snippet: snippet(&hit.text_content, 160),
    }
}

pub fn citations_for_hits(hits: &[SearchHit]) -> Vec<Citation> {
    hits.iter().map(citation_for_hit).collect()
}

fn snippet(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(max_chars).collect();
    format!("{}...", truncated.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::{ChunkId, DocumentId};

    fn hit(text: &str) -> SearchHit {
        SearchHit {
            chunk_id: ChunkId(1),
            document_id: DocumentId(2),
            text_content: text.to_string(),
            page_or_location_ref: "3".to_string(),
            score: 1.0,
        }
    }

    #[test]
    fn citation_carries_document_chunk_and_location() {
        let citation = citation_for_hit(&hit("some text"));
        assert_eq!(citation.document_id, DocumentId(2));
        assert_eq!(citation.chunk_id, ChunkId(1));
        assert_eq!(citation.location_ref, "3");
    }

    #[test]
    fn short_text_is_not_truncated() {
        let citation = citation_for_hit(&hit("short text"));
        assert_eq!(citation.snippet, "short text");
    }

    #[test]
    fn long_text_is_truncated_with_an_ellipsis() {
        let long_text = "word ".repeat(100);
        let citation = citation_for_hit(&hit(&long_text));
        assert!(citation.snippet.ends_with("..."));
        assert!(citation.snippet.chars().count() <= 163);
    }

    #[test]
    fn citations_for_hits_preserves_order() {
        let hits = vec![hit("first"), hit("second")];
        let citations = citations_for_hits(&hits);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].snippet, "first");
        assert_eq!(citations[1].snippet, "second");
    }
}
