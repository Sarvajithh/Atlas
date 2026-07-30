//! Context Builder (§39). Sits between Retrieval and the Tutor/Reasoning
//! Engines: ranking, compression, deduplication, token budgeting, ordering,
//! citation preparation, and context validation (§39.1). Refines §15's
//! pipeline without renaming or removing any of its steps (§39.2).

use atlas_types::chunk::Chunk;
use atlas_utils::AppError;

/// A chunk annotated with citation metadata (§39.1 "citation preparation"),
/// ready for the Prompt Builder (§40).
pub struct AssembledContext {
    pub chunks: Vec<Chunk>,
}

pub struct ContextBuilder {
    /// Token budget strategy is configuration-driven (§39.1), not hardcoded.
    max_context_tokens: u32,
}

impl ContextBuilder {
    pub fn new(max_context_tokens: u32) -> Self {
        Self { max_context_tokens }
    }

    pub fn max_context_tokens(&self) -> u32 {
        self.max_context_tokens
    }

    /// Assemble ranked/compressed/deduplicated context from retrieved chunks
    /// (§39.1). Concrete ranking/compression logic deferred to a future
    /// milestone.
    pub fn assemble(&self, chunks: Vec<Chunk>) -> Result<AssembledContext, AppError> {
        Ok(AssembledContext { chunks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::ids::{ChunkId, DocumentId};

    fn sample_chunk(sequence_index: u32, text: &str) -> Chunk {
        Chunk {
            id: ChunkId(sequence_index as i64),
            document_id: DocumentId(1),
            sequence_index,
            text_content: text.to_string(),
            page_or_location_ref: "1".to_string(),
            token_count: 10,
            parser_version: "1".to_string(),
        }
    }

    #[test]
    fn max_context_tokens_is_configurable_not_hardcoded() {
        assert_eq!(ContextBuilder::new(2048).max_context_tokens(), 2048);
        assert_eq!(ContextBuilder::new(8192).max_context_tokens(), 8192);
    }

    #[test]
    fn assemble_preserves_all_input_chunks() {
        let builder = ContextBuilder::new(4096);
        let chunks = vec![sample_chunk(0, "a"), sample_chunk(1, "b")];
        let assembled = builder.assemble(chunks).unwrap();
        assert_eq!(assembled.chunks.len(), 2);
    }

    #[test]
    fn assemble_of_empty_input_is_empty_context() {
        let builder = ContextBuilder::new(4096);
        let assembled = builder.assemble(Vec::new()).unwrap();
        assert!(assembled.chunks.is_empty());
    }
}
