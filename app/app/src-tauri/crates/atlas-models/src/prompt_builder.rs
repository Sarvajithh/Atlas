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
use atlas_types::memory::WeakTopic;

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

/// Default instruction section for the Quiz Generator (§ Learning
/// subsystem). Deliberately pins down the exact JSON shape
/// `study_output::parse_quiz_response` expects -- `options` as an array of
/// strings, `correct_answer` copied verbatim from one of them (this is
/// checked by validation, not assumed), and `source_citations` referencing
/// the `[n]` markers in the WORKSPACE CONTEXT section below, when the
/// question draws on retrieved material.
const DEFAULT_QUIZ_INSTRUCTION: &str = "You are Atlas's Quiz Generator. Using the workspace context below as your \
primary source (supplementing with well-established general knowledge only where the context doesn't cover the \
requested topic), produce quiz questions.\n\
Respond with ONLY a single JSON object, no markdown code fences, no commentary, matching exactly this shape:\n\
{\"topic\": string, \"questions\": [{\"question\": string, \"options\": [string, ...] (at least 2), \
\"correct_answer\": string (must be copied verbatim from one of \\\"options\\\"), \"source_citations\": [string, ...] \
(the [n] markers this question draws on, or an empty array if none)}]}";

/// Default instruction section for the Flashcard Generator.
const DEFAULT_FLASHCARD_INSTRUCTION: &str = "You are Atlas's Flashcard Generator. Using the workspace context below \
as your primary source (supplementing with well-established general knowledge only where the context doesn't cover \
the requested topic), produce flashcards -- each a pedagogical front/back pair, not a verbatim quote of the source.\n\
Respond with ONLY a single JSON object, no markdown code fences, no commentary, matching exactly this shape:\n\
{\"topic\": string, \"cards\": [{\"front\": string, \"back\": string, \"source_citations\": [string, ...] (the [n] \
markers this card draws on, or an empty array if none)}]}";

/// Default instruction section for the Revision Planner. Explicitly tells
/// the model to prioritize by the supplied weak-topic accuracy data rather
/// than guessing, matching the implementation plan's requirement that the
/// planner consume the computed aggregate as structured input.
const DEFAULT_REVISION_PLAN_INSTRUCTION: &str = "You are Atlas's Revision Planner. You are given real, computed \
weak-topic data below (accuracy from actually-recorded quiz attempts, not a guess). Produce a revision plan that \
prioritizes the lowest-accuracy topics first.\n\
Respond with ONLY a single JSON object, no markdown code fences, no commentary, matching exactly this shape:\n\
{\"items\": [{\"topic\": string, \"recommendation\": string, \"priority\": integer (1 = most urgent, increasing \
number = less urgent; the weakest topic below should generally get priority 1)}]}";

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

    /// Fetches `key` from settings, falling back to `default` on either an
    /// unset/blank value or a settings-read failure -- the exact
    /// `system_prompt`-style fallback pattern, generalized so each
    /// structured-output template below doesn't reimplement it.
    fn setting_or_default(&self, key: &str, default: &str) -> String {
        match self.settings.get_global(key) {
            Ok(Some(entry)) if !entry.value.trim().is_empty() => entry.value,
            Ok(_) => default.to_string(),
            Err(e) => {
                atlas_utils::log_warn!("[PromptBuilder] failed to read {key}, using default: {}", e.message);
                default.to_string()
            }
        }
    }

    /// Render retrieved context hits as numbered `[n]` blocks (§39.1
    /// "citation preparation"), shared by every prompt-building method that
    /// takes an `AssembledContext` so the marker scheme stays identical
    /// across chat, quiz, and flashcard prompts.
    fn context_block(context: &AssembledContext) -> String {
        if context.hits.is_empty() {
            "(No relevant workspace material was retrieved -- generate from general knowledge and note that no source material was found.)".to_string()
        } else {
            context
                .hits
                .iter()
                .enumerate()
                .map(|(idx, hit)| format!("[{}] {}", idx + 1, hit.text_content))
                .collect::<Vec<_>>()
                .join("\n\n")
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

        let context_block = Self::context_block(&context);

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

    /// Settings key for the Quiz Generator's structured-output instruction
    /// section, overridable the same way as `SYSTEM_PROMPT_SETTING_KEY`.
    pub const QUIZ_PROMPT_SETTING_KEY: &'static str = "learning.quiz_prompt_template";

    /// Settings key for the Flashcard Generator's structured-output
    /// instruction section.
    pub const FLASHCARD_PROMPT_SETTING_KEY: &'static str = "learning.flashcard_prompt_template";

    /// Settings key for the Revision Planner's structured-output
    /// instruction section.
    pub const REVISION_PLAN_PROMPT_SETTING_KEY: &'static str = "learning.revision_plan_prompt_template";

    /// Assemble a Quiz Generator prompt (§ Learning subsystem). Instructs
    /// the model to return JSON matching `study_output::RawQuiz`'s shape
    /// exactly, so `study_output::parse_quiz_response` can parse it without
    /// the two ever silently drifting apart. `num_questions` is a request,
    /// not a hard guarantee the model will produce exactly that many --
    /// validation in `study_output` only requires at least one.
    pub fn build_quiz_prompt(&self, topic: &str, context: AssembledContext, num_questions: u32) -> ResolvedPrompt {
        let instruction = self.setting_or_default(Self::QUIZ_PROMPT_SETTING_KEY, DEFAULT_QUIZ_INSTRUCTION);
        let context_block = Self::context_block(&context);
        let content = format!(
            "SYSTEM\n\n{instruction}\n\n\
             ---\n\n\
             WORKSPACE CONTEXT\n\n{context_block}\n\n\
             ---\n\n\
             REQUEST\n\nGenerate {num_questions} quiz question(s) on the topic \"{topic}\".\n\n\
             ---\n\n\
             ANSWER\n\nRespond with the JSON object only, matching the schema above."
        );
        ResolvedPrompt::text(content)
    }

    /// Assemble a Flashcard Generator prompt, same schema-instruction
    /// pattern as `build_quiz_prompt`.
    pub fn build_flashcard_prompt(&self, topic: &str, context: AssembledContext, num_cards: u32) -> ResolvedPrompt {
        let instruction = self.setting_or_default(Self::FLASHCARD_PROMPT_SETTING_KEY, DEFAULT_FLASHCARD_INSTRUCTION);
        let context_block = Self::context_block(&context);
        let content = format!(
            "SYSTEM\n\n{instruction}\n\n\
             ---\n\n\
             WORKSPACE CONTEXT\n\n{context_block}\n\n\
             ---\n\n\
             REQUEST\n\nGenerate {num_cards} flashcard(s) on the topic \"{topic}\".\n\n\
             ---\n\n\
             ANSWER\n\nRespond with the JSON object only, matching the schema above."
        );
        ResolvedPrompt::text(content)
    }

    /// Assemble a Revision Planner prompt. Unlike the quiz/flashcard
    /// prompts, this takes no retrieved `AssembledContext` -- its input is
    /// the *computed* weak-topic aggregate (§ analytics_repository), not
    /// document context, per the implementation plan's requirement that
    /// the planner "consumes the weak-topic aggregate as structured input
    /// to its prompt ... rather than operating blind."
    pub fn build_revision_plan_prompt(&self, weak_topics: &[WeakTopic]) -> ResolvedPrompt {
        let instruction = self.setting_or_default(Self::REVISION_PLAN_PROMPT_SETTING_KEY, DEFAULT_REVISION_PLAN_INSTRUCTION);
        let weak_topics_block = if weak_topics.is_empty() {
            "(No weak-topic data is available yet -- no quiz attempts have been recorded. Produce a short general-review plan and say so.)".to_string()
        } else {
            weak_topics
                .iter()
                .map(|t| {
                    format!(
                        "- topic: \"{}\", correct: {}, incorrect: {}, accuracy: {:.0}%",
                        t.topic,
                        t.correct_count,
                        t.incorrect_count,
                        t.accuracy * 100.0
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let content = format!(
            "SYSTEM\n\n{instruction}\n\n\
             ---\n\n\
             WEAK-TOPIC DATA (computed from recorded quiz attempts, lowest accuracy is weakest)\n\n{weak_topics_block}\n\n\
             ---\n\n\
             ANSWER\n\nRespond with the JSON object only, matching the schema above."
        );
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

    // ---- Quiz / Flashcard / Revision Planner prompts ----

    #[test]
    fn build_quiz_prompt_includes_topic_count_and_schema_instruction() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build_quiz_prompt("Photosynthesis", context(&["light reactions occur in the thylakoid"]), 5);
        assert!(prompt.content.contains("Photosynthesis"));
        assert!(prompt.content.contains("Generate 5 quiz question"));
        assert!(prompt.content.contains("correct_answer"));
        assert!(prompt.content.contains("[1] light reactions occur in the thylakoid"));
    }

    #[test]
    fn build_quiz_prompt_of_empty_context_still_says_so() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build_quiz_prompt("t", context(&[]), 3);
        assert!(prompt.content.contains("No relevant workspace material"));
    }

    #[test]
    fn build_flashcard_prompt_includes_topic_count_and_schema_instruction() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build_flashcard_prompt("Cell Biology", context(&["ribosomes synthesize protein"]), 4);
        assert!(prompt.content.contains("Cell Biology"));
        assert!(prompt.content.contains("Generate 4 flashcard"));
        assert!(prompt.content.contains("\"front\""));
        assert!(prompt.content.contains("[1] ribosomes synthesize protein"));
    }

    #[test]
    fn build_revision_plan_prompt_includes_weak_topic_data() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let weak_topics = vec![atlas_types::memory::WeakTopic {
            topic: "Thermodynamics".to_string(),
            correct_count: 2,
            incorrect_count: 8,
            accuracy: 0.2,
        }];
        let prompt = builder.build_revision_plan_prompt(&weak_topics);
        assert!(prompt.content.contains("Thermodynamics"));
        assert!(prompt.content.contains("accuracy: 20%"));
        assert!(prompt.content.contains("\"priority\""));
    }

    #[test]
    fn build_revision_plan_prompt_of_no_weak_topics_says_so_rather_than_fabricating() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build_revision_plan_prompt(&[]);
        assert!(prompt.content.contains("No weak-topic data is available"));
    }

    #[test]
    fn quiz_and_flashcard_prompts_respect_settings_overrides() {
        let provider = LayeredSettingsProvider::new();
        provider
            .set_in_layer(
                atlas_config::hierarchy::ConfigLayer::Runtime,
                atlas_types::settings::SettingEntry {
                    key: PromptBuilder::QUIZ_PROMPT_SETTING_KEY.to_string(),
                    value: "CUSTOM QUIZ INSTRUCTION".to_string(),
                    value_type: "string".to_string(),
                    scope: atlas_types::settings::SettingsScope::Global,
                    workspace_id: None,
                    updated_at: "1970-01-01T00:00:00Z".to_string(),
                },
            )
            .unwrap();
        let builder = PromptBuilder::new(Arc::new(provider));
        let prompt = builder.build_quiz_prompt("t", context(&[]), 1);
        assert!(prompt.content.contains("CUSTOM QUIZ INSTRUCTION"));
    }
}
