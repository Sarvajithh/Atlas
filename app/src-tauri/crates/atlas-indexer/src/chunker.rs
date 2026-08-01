//! Chunking Engine (§14, §18, §33.3). Runs downstream of the Parser Layer
//! (§36.3: "Parsers MUST NOT perform chunking") -- it turns a `ParsedDocument`
//! (§35.1) into normalized `Chunk` rows ready for the Embedding Engine and
//! keyword search (§18). Chunk size/overlap are configuration (Governing
//! Principle), passed in by the caller rather than hardcoded here.

use atlas_types::chunk::Chunk;
use atlas_types::document::ParsedDocument;
use atlas_types::ids::{ChunkId, DocumentId};

/// Chunking strategy parameters (§18: "Chunking strategy ... is
/// configuration bound to the Embedding Engine ... not hardcoded in the
/// pipeline logic"). `max_tokens`/`overlap_tokens` are approximated by
/// whitespace-delimited word counts, avoiding a tokenizer dependency this
/// milestone doesn't need for correctness of the pipeline shape.
#[derive(Debug, Clone, Copy)]
pub struct ChunkingConfig {
    pub max_tokens: u32,
    pub overlap_tokens: u32,
}

impl ChunkingConfig {
    pub fn new(max_tokens: u32, overlap_tokens: u32) -> Self {
        Self {
            max_tokens,
            overlap_tokens,
        }
    }
}

impl Default for ChunkingConfig {
    /// A sane, documented default (§37.2's "default assignments ship as
    /// default configuration values, not code constants" pattern, applied
    /// here to chunk sizing) -- callers needing something else pass their
    /// own `ChunkingConfig` explicitly.
    fn default() -> Self {
        Self {
            max_tokens: 256,
            overlap_tokens: 32,
        }
    }
}

/// The version tag stamped onto every `Chunk::parser_version` this engine
/// produces (§22 cache invalidation key: "source file content hash +
/// parser/engine version tag"). Bump when the chunking algorithm changes.
pub const CHUNKER_VERSION: &str = "chunker-v1";

/// Split a parsed document's blocks into overlapping, token-budgeted chunks
/// (§18). Each block's `location_ref` is preserved onto every chunk it
/// contributes to, so citations (§39.1, §44.1) can still point at the
/// right page/location even after a block is split across chunks.
pub fn chunk_document(document_id: DocumentId, parsed: &ParsedDocument, config: ChunkingConfig) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut sequence_index: u32 = 0;

    for block in &parsed.blocks {
        if block.text_content.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = block.text_content.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        let step = config
            .max_tokens
            .saturating_sub(config.overlap_tokens)
            .max(1) as usize;
        let window = config.max_tokens.max(1) as usize;

        let mut start = 0usize;
        loop {
            let end = (start + window).min(words.len());
            let text_content = words[start..end].join(" ");
            chunks.push(Chunk {
                id: ChunkId(0),
                document_id,
                sequence_index,
                text_content,
                page_or_location_ref: block.location_ref.page_or_location.clone(),
                token_count: (end - start) as u32,
                parser_version: CHUNKER_VERSION.to_string(),
            });
            sequence_index += 1;

            if end >= words.len() {
                break;
            }
            start += step;
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::document::{Block, BlockType, DocumentMetadata, LocationRef};

    fn sample_document(text: &str) -> ParsedDocument {
        ParsedDocument {
            metadata: DocumentMetadata {
                title: "Sample".to_string(),
                file_type: "md".to_string(),
                content_hash: "abc".to_string(),
            },
            blocks: vec![Block {
                block_type: BlockType::Paragraph,
                location_ref: LocationRef {
                    page_or_location: "1".to_string(),
                },
                text_content: text.to_string(),
            }],
        }
    }

    #[test]
    fn short_block_becomes_a_single_chunk() {
        let doc = sample_document("one two three");
        let chunks = chunk_document(DocumentId(1), &doc, ChunkingConfig::new(10, 2));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text_content, "one two three");
        assert_eq!(chunks[0].sequence_index, 0);
    }

    #[test]
    fn long_block_is_split_with_overlap() {
        let words: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let doc = sample_document(&words.join(" "));
        let chunks = chunk_document(DocumentId(1), &doc, ChunkingConfig::new(10, 3));
        assert!(chunks.len() > 1);
        // Overlap: the last words of chunk 0 should reappear at the start
        // of chunk 1.
        let first_words: Vec<&str> = chunks[0].text_content.split_whitespace().collect();
        let second_words: Vec<&str> = chunks[1].text_content.split_whitespace().collect();
        assert_eq!(&first_words[first_words.len() - 3..], &second_words[..3]);
    }

    #[test]
    fn empty_blocks_are_skipped() {
        let doc = sample_document("   ");
        let chunks = chunk_document(DocumentId(1), &doc, ChunkingConfig::default());
        assert!(chunks.is_empty());
    }

    #[test]
    fn location_ref_is_preserved_onto_every_chunk() {
        let words: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let doc = sample_document(&words.join(" "));
        let chunks = chunk_document(DocumentId(7), &doc, ChunkingConfig::new(10, 2));
        assert!(chunks.iter().all(|c| c.page_or_location_ref == "1"));
        assert!(chunks.iter().all(|c| c.document_id == DocumentId(7)));
    }
}
