//! Prompt Builder (§40). No Engine formats its own prompt; every Engine
//! receives a fully-assembled prompt from here. Templates are configuration
//! data (resolved via atlas-config), never string-literal constants inside
//! Engine code (§40.1, Governing Principle).

use std::sync::Arc;

use atlas_config::SettingsProvider;

use crate::context_builder::AssembledContext;
use crate::engine::ResolvedPrompt;

pub struct PromptBuilder {
    settings: Arc<dyn SettingsProvider>,
}

impl PromptBuilder {
    pub fn new(settings: Arc<dyn SettingsProvider>) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    /// Resolve the configured template and assemble the final prompt
    /// (§40.1). Template resolution and assembly logic deferred to a future
    /// milestone.
    pub fn build(&self, context: AssembledContext) -> ResolvedPrompt {
        let content = context
            .chunks
            .iter()
            .map(|c| c.text_content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        ResolvedPrompt { content }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_config::hierarchy::LayeredSettingsProvider;
    use atlas_types::chunk::Chunk;
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
    fn build_joins_chunk_text_in_order() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let context = AssembledContext {
            chunks: vec![sample_chunk(0, "first"), sample_chunk(1, "second")],
        };
        let prompt = builder.build(context);
        assert_eq!(prompt.content, "first\nsecond");
    }

    #[test]
    fn build_of_empty_context_is_empty_prompt() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build(AssembledContext { chunks: Vec::new() });
        assert_eq!(prompt.content, "");
    }
}
