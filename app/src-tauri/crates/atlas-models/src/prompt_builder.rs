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
/// Appended to every SYSTEM prompt (default and Research Mode variants
/// alike). Real production bug this closes: nothing anywhere in this
/// module told the model to use LaTeX delimiters for math, so models
/// simply wrote plain text ("x^2", unicode symbols, etc.) with no `$`/`$$`
/// around it -- the frontend's KaTeX renderer (`remark-math`/
/// `rehype-katex`, wired in both the Assistant chat panel and the
/// in-document Markdown viewer) had nothing to render, which looked
/// indistinguishable from "LaTeX rendering is broken" even though the
/// renderer itself worked correctly the whole time. Explicit inline
/// (`$...$`) vs display (`$$...$$`) guidance, since models are otherwise
/// inconsistent about which one to reach for.
const MATH_FORMATTING_INSTRUCTION: &str = "Formatting: write ALL mathematical notation -- equations, formulas, variables, symbols -- using LaTeX delimiters, since the answer is rendered with a math renderer that requires them. \
Use single dollar signs for inline math within a sentence, e.g. $x^2 + y^2 = z^2$, and double dollar signs on their own line for standalone/display equations, e.g. $$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$. \
Never write mathematical notation as plain unformatted text (e.g. \"x^2\" or \"integral of x\") when LaTeX would express it -- always wrap it in $ or $$.";

const DEFAULT_SYSTEM_PROMPT: &str = "You are Atlas, an expert AI tutor.\n\
Treat the retrieved workspace context below as authoritative course material and your primary source.\n\
However, you are NOT limited to it: whenever the workspace context does not fully answer the question, \
use your own well-established general knowledge to explain the concept -- never contradict the workspace, \
but do supplement it freely for facts, history, or definitions that are well established and simply weren't retrieved.\n\
Never simply repeat or quote retrieved passages back verbatim. Teach naturally: explain ideas, give intuition, \
connect related concepts, compare similar topics, and provide examples where they help understanding.\n\
Cite workspace evidence inline using [1], [2], ... matching the numbered context below, wherever you actually draw on it. \
If your answer comes entirely from general knowledge rather than the workspace context, say so plainly.";

/// Which Research Mode task the prompt is being built for (§ objective
/// "literature review support, paper comparison"). Both reuse the same
/// underlying synthesize-across-sources machinery -- this only changes the
/// system framing, not the retrieval/context-assembly pipeline, matching
/// the objective's "reuse Retriever/ContextBuilder/PromptBuilder, extended
/// not replaced".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchPromptMode {
    /// Synthesize across every retrieved source into one coherent answer.
    LiteratureReview,
    /// Explicitly structure the answer around agreements/disagreements/
    /// gaps between the retrieved sources, rather than one blended
    /// narrative.
    PaperComparison,
}

const RESEARCH_LITERATURE_REVIEW_SYSTEM_PROMPT: &str = "You are Atlas, an AI research assistant helping a student conduct a literature review.\n\
You have been given retrieved passages from MULTIPLE documents, possibly spanning multiple workspaces -- each passage is labeled with its source.\n\
Synthesize across all of them into one coherent answer: identify where sources agree, where they add complementary detail, and where they genuinely conflict -- say so explicitly if they do.\n\
Never present a single source's view as the consensus if the other retrieved sources don't support it.\n\
Cite every claim inline using [1], [2], ... matching the numbered, source-labeled context below.\n\
If the retrieved context does not fully answer the question, use general knowledge to fill gaps, but never contradict what the sources actually say, and say plainly when you are going beyond them.\n\
Never fabricate a relationship between sources that the retrieved text doesn't actually support.";

const RESEARCH_PAPER_COMPARISON_SYSTEM_PROMPT: &str = "You are Atlas, an AI research assistant helping a student compare multiple sources.\n\
You have been given retrieved passages from MULTIPLE documents, possibly spanning multiple workspaces -- each passage is labeled with its source.\n\
Structure your answer around the comparison itself: what each source claims on the question asked, where they agree, where they disagree, and what each is missing that another covers.\n\
Do not blend the sources into one undifferentiated narrative -- keep it clear which source each specific claim comes from.\n\
Cite every claim inline using [1], [2], ... matching the numbered, source-labeled context below.\n\
Never fabricate an agreement or disagreement between sources that the retrieved text doesn't actually support.";

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
            "SYSTEM\n\n{system}\n\n{math_instruction}\n\n\
             ---\n\n\
             WORKSPACE CONTEXT\n\n{context_block}\n\n\
             ---\n\n\
             USER QUESTION\n\n{query}\n\n\
             ---\n\n\
             ANSWER\n\nBegin naturally.",
            system = self.system_prompt(),
            math_instruction = MATH_FORMATTING_INSTRUCTION,
        );

        atlas_utils::log_info!("[PromptBuilder] exited, prompt size = {} chars", content.len());
        ResolvedPrompt::text(content)
    }

    /// Research Mode's variant of `build` (§ objective "literature review
    /// support, paper comparison"): same structured SYSTEM / CONTEXT /
    /// QUESTION / ANSWER shape, but (a) uses a synthesis-across-sources
    /// system prompt instead of the tutor persona, and (b) labels each
    /// numbered context block with which document/workspace it came from
    /// (`source_labels`, keyed by `document_id.0`), so the model -- and a
    /// reader checking the citations -- can actually tell sources apart
    /// instead of seeing an anonymous chunk dump. A `document_id` with no
    /// entry in `source_labels` falls back to `"document #<id>"` rather
    /// than silently omitting the label.
    pub fn build_research(
        &self,
        query: &str,
        context: AssembledContext,
        mode: ResearchPromptMode,
        source_labels: &std::collections::HashMap<i64, String>,
    ) -> ResolvedPrompt {
        let context_block = if context.hits.is_empty() {
            "(No relevant material was retrieved from the selected workspaces for this question -- answer from general knowledge and say so.)".to_string()
        } else {
            context
                .hits
                .iter()
                .enumerate()
                .map(|(idx, hit)| {
                    let label = source_labels
                        .get(&hit.document_id.0)
                        .cloned()
                        .unwrap_or_else(|| format!("document #{}", hit.document_id.0));
                    format!("[{}] (source: {label})\n{}", idx + 1, hit.text_content)
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        let system = match mode {
            ResearchPromptMode::LiteratureReview => RESEARCH_LITERATURE_REVIEW_SYSTEM_PROMPT,
            ResearchPromptMode::PaperComparison => RESEARCH_PAPER_COMPARISON_SYSTEM_PROMPT,
        };

        let content = format!(
            "SYSTEM\n\n{system}\n\n{math_instruction}\n\n\
             ---\n\n\
             RETRIEVED SOURCES\n\n{context_block}\n\n\
             ---\n\n\
             RESEARCH QUESTION\n\n{query}\n\n\
             ---\n\n\
             ANSWER\n\nBegin naturally.",
            math_instruction = MATH_FORMATTING_INSTRUCTION,
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
    fn build_instructs_the_model_to_use_latex_delimiters_for_math() {
        // Regression test for a real production bug: nothing in the
        // prompt ever told the model to wrap math in $/$$ delimiters, so
        // it wrote plain unformatted text instead -- indistinguishable
        // from "the KaTeX renderer is broken" even though the renderer
        // (already wired in both the Assistant panel and the Markdown
        // document viewer) worked correctly the whole time.
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build("q", context(&["c"]));
        assert!(prompt.content.contains("LaTeX"));
        assert!(prompt.content.contains("$x^2"));
    }

    #[test]
    fn build_research_also_instructs_the_model_to_use_latex_delimiters() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build_research(
            "q",
            context(&["c"]),
            ResearchPromptMode::LiteratureReview,
            &std::collections::HashMap::new(),
        );
        assert!(prompt.content.contains("LaTeX"));
    }

    #[test]
    fn build_of_empty_context_still_includes_query_and_says_so() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let prompt = builder.build("a question with no retrieved context", context(&[]));
        assert!(prompt.content.contains("a question with no retrieved context"));
        assert!(prompt.content.contains("No relevant workspace material"));
    }

    // ---- Research Mode: build_research ----

    #[test]
    fn build_research_labels_each_source_and_includes_the_question() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let mut labels = std::collections::HashMap::new();
        labels.insert(1, "Workspace A / paper1.pdf".to_string());
        let prompt = builder.build_research(
            "compare the two approaches",
            context(&["approach one details"]),
            ResearchPromptMode::LiteratureReview,
            &labels,
        );
        assert!(prompt.content.contains("Workspace A / paper1.pdf"));
        assert!(prompt.content.contains("compare the two approaches"));
        assert!(prompt.content.contains("[1]"));
    }

    #[test]
    fn build_research_falls_back_to_a_generic_label_when_none_is_provided() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let labels = std::collections::HashMap::new();
        let prompt = builder.build_research(
            "q",
            context(&["c"]),
            ResearchPromptMode::LiteratureReview,
            &labels,
        );
        assert!(prompt.content.contains("document #1"));
    }

    #[test]
    fn build_research_paper_comparison_mode_uses_the_comparison_system_prompt() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let labels = std::collections::HashMap::new();
        let prompt = builder.build_research("q", context(&["c"]), ResearchPromptMode::PaperComparison, &labels);
        assert!(prompt.content.to_lowercase().contains("compare"));
    }

    #[test]
    fn build_research_of_empty_context_says_so_and_still_includes_query() {
        let builder = PromptBuilder::new(Arc::new(LayeredSettingsProvider::new()));
        let labels = std::collections::HashMap::new();
        let prompt = builder.build_research(
            "a research question",
            context(&[]),
            ResearchPromptMode::LiteratureReview,
            &labels,
        );
        assert!(prompt.content.contains("a research question"));
        assert!(prompt.content.contains("No relevant material"));
    }
}
