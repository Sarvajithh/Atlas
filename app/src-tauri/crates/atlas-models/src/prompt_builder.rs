//! Prompt Builder (§40). No Engine formats its own prompt; every Engine
//! receives a fully-assembled prompt from here. Templates are configuration
//! data (resolved via atlas-config), never string-literal constants inside
//! Engine code (§40.1, Governing Principle).
//!
//! Root-cause fix (Part 1 of the prompt-quality audit): `build` previously
//! took only `context` -- the user's actual question never reached this
//! module, and therefore never reached the model. What Ollama received was
//! a bare, unlabeled dump of retrieved chunks with no question and no
//! instruction attached, which is why the assistant behaved like a
//! citation/extraction engine instead of a tutor: there was nothing in the
//! prompt telling it to explain, teach, or even what was being asked.
//! `build` now takes `query: &str` and assembles a structured, sectioned
//! prompt (SYSTEM / WORKSPACE CONTEXT / USER QUESTION / ANSWER) instead of
//! plain concatenation.

use std::sync::Arc;

use atlas_config::SettingsProvider;

use crate::context_builder::AssembledContext;
use crate::engine::ResolvedPrompt;

pub struct PromptBuilder {
    settings: Arc<dyn SettingsProvider>,
}

/// Settings key for the SYSTEM section template (§40.1, §23 "never a
/// hardcoded... anything the user might reasonably want to change" --
/// operators/advanced users can override the tutor's persona/instructions
/// without a code change). Falls back to `DEFAULT_SYSTEM_PROMPT` when unset,
/// same pattern as `FALLBACK_NUM_CTX` in `ollama.rs`: a named, documented
/// default rather than a silent guess.
const SYSTEM_PROMPT_SETTING_KEY: &str = "assistant.system_prompt_template";

/// Default SYSTEM section (Part 2 of the prompt-quality audit's required
/// structure) used when no override is configured via
/// `SYSTEM_PROMPT_SETTING_KEY`. Instructs the model to teach, not extract:
/// treat workspace context as authoritative but not a hard limit, use
/// general knowledge to fill gaps, never simply repeat retrieved passages,
/// and cite inline with `[n]` markers matching `context.citations`'
/// ordering (§44.1).
const DEFAULT_SYSTEM_PROMPT: &str = "You are Atlas, an expert AI tutor.\n\
Treat the retrieved workspace context below as authoritative course material and your primary source.\n\
However, you are NOT limited to it: whenever the workspace context does not fully answer the question, \
use your own well-established general knowledge to explain the concept -- never contradict the workspace, \
but do supplement it freely for facts, history, or definitions that are well established and simply weren't retrieved.\n\
Never simply repeat or quote retrieved passages back verbatim. Teach naturally: explain ideas, give intuition, \
connect related concepts, compare similar topics, and provide examples where they help understanding.\n\
Cite workspace evidence inline using [1], [2], ... matching the numbered context below, wherever you actually draw on it. \
If your answer comes entirely from general knowledge rather than the workspace context, say so plainly.";

impl PromptBuilder {
    pub fn new(settings: Arc<dyn SettingsProvider>) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    fn system_prompt(&self) -> String {
        match self.settings.get_global(SYSTEM_PROMPT_SETTING_KEY) {
            Ok(Some(entry)) if !entry.value.trim().is_empty() => entry.value,
            Ok(_) => DEFAULT_SYSTEM_PROMPT.to_string(),
            Err(e) => {
                // §45.1 recoverable: a settings-read failure for an
                // optional template override shouldn't break the whole
                // prompt -- fall back to the documented default and note
                // why, rather than propagating the error up through
                // chat_stream.
                atlas_utils::log_warn!(
                    "[PromptBuilder] failed to read {SYSTEM_PROMPT_SETTING_KEY}, using default: {}",
                    e.message
                );
                DEFAULT_SYSTEM_PROMPT.to_string()
            }
        }
    }

    /// Assemble the final prompt (§40.1) from the user's `query` plus
    /// context chunks with citation markers (§39.1 "citation
    /// preparation"). Structured into clearly separated sections (Part 2
    /// of the prompt-quality audit) rather than plain concatenation, so
    /// the model receives an actual instruction, the real question, and
    /// labeled context -- not just an unlabeled chunk dump. Each chunk is
    /// rendered with an inline `[n]` marker matching its position in
    /// `context.citations`, so a downstream Engine's answer can reference
    /// `[n]` and the UI can resolve that back to a `Citation` (§44.1) for
    /// click-through to the source document.
    pub fn build(&self, query: &str, context: AssembledContext) -> ResolvedPrompt {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        atlas_utils::log_info!(
            "[PromptBuilder] entered with {} context hits, query_chars={}",
            context.hits.len(),
            query.len()
        );

        let context_block = if context.hits.is_empty() {
            "(No relevant workspace material was retrieved for this question -- answer from general knowledge and say so.)".to_string()
        } else {
            context
                .hits
                .iter()
                .enumerate()
                .map(|(idx, hit)| format!("[{}] {}", idx + 1, hit.text_content))
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        let content = format!(
            "SYSTEM\n\n{system}\n\n\
             ---\n\n\
             WORKSPACE CONTEXT\n\n{context_block}\n\n\
             ---\n\n\
             USER QUESTION\n\n{query}\n\n\
             ---\n\n\
             ANSWER\n\nBegin naturally.",
            system = self.system_prompt(),
        );

        atlas_utils::log_info!("[PromptBuilder] exited, prompt size = {} chars", content.len());
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
        let prompt = builder.build("what is it", context(&["first", "second"]));
        assert!(prompt.content.contains("[1] first"));
        assert!(prompt.content.contains("[2] second"));
    }

    /// Root-cause regression test: the bug this fix closes was that the
    /// user's question never reached the prompt at all. This test fails
    /// against the old `build(context)` signature/behavior and passes only
    /// once `query` is actually threaded into the output.
    #[test]
    fn build_includes_the_users_actual_question() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build("What is differential privacy?", context(&["some context"]));
        assert!(prompt.content.contains("What is differential privacy?"));
    }

    #[test]
    fn build_includes_a_system_instruction_section() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build("q", context(&["c"]));
        assert!(prompt.content.contains("SYSTEM"));
        assert!(prompt.content.to_lowercase().contains("tutor"));
    }

    #[test]
    fn build_of_empty_context_still_includes_query_and_says_so() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build("a question with no retrieved context", context(&[]));
        assert!(prompt.content.contains("a question with no retrieved context"));
        assert!(prompt.content.contains("No relevant workspace material"));
    }
}
