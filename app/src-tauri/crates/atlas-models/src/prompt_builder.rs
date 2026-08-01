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

    /// Assemble the final prompt from context chunks plus citation markers
    /// (§40.1, §39.1 "citation preparation"). Each chunk is rendered with
    /// an inline `[n]` marker matching its position in
    /// `context.citations`, so a downstream Engine's answer can reference
    /// `[n]` and the UI can resolve that back to a `Citation` (§44.1) for
    /// click-through to the source document.
    pub fn build(&self, context: AssembledContext) -> ResolvedPrompt {
        let content = context
            .hits
            .iter()
            .enumerate()
            .map(|(idx, hit)| format!("[{}] {}", idx + 1, hit.text_content))
            .collect::<Vec<_>>()
            .join("\n\n");
        ResolvedPrompt::text(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_config::hierarchy::LayeredSettingsProvider;
    use atlas_types::ids::{ChunkId, DocumentId};
    use atlas_types::retrieval::SearchHit;

    fn context(texts: &[&str]) -> AssembledContext {
        let hits: Vec<SearchHit> = texts
            .iter()
            .enumerate()
            .map(|(idx, text)| SearchHit {
                chunk_id: ChunkId(idx as i64 + 1),
                document_id: DocumentId(1),
                text_content: text.to_string(),
                page_or_location_ref: "1".to_string(),
                score: 0.0,
            })
            .collect();
        AssembledContext {
            citations: crate::citation::citations_for_hits(&hits),
            total_tokens: 0,
            hits,
        }
    }

    #[test]
    fn build_numbers_each_chunk_as_an_inline_citation_marker() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build(context(&["first", "second"]));
        assert_eq!(prompt.content, "[1] first\n\n[2] second");
    }

    #[test]
    fn build_of_empty_context_is_empty_prompt() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build(context(&[]));
        assert_eq!(prompt.content, "");
    }
}
