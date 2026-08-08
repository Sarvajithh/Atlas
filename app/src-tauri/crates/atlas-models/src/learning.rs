//! Learning artifact generation: Mind Maps, Formula Sheets, Study Guides.
//!
//! These reuse the same "structured-output over a Reasoning-role prompt"
//! pattern `atlas-graph::extraction::ConceptExtractor` established for
//! Concept Graph extraction (build a prompt instructing the model to
//! return ONLY a JSON object in a fixed shape, strip an accidental
//! markdown code fence, `serde_json::from_str` into a small typed struct)
//! rather than the older Quiz/Flashcard/Revision Planner pattern of
//! returning raw freeform model text with no schema at all (see README
//! "Quiz / Flashcard / Revision Planner generation depth").
//!
//! This module only builds prompts and parses responses -- it has no
//! knowledge of retrieval, the Concept Graph, or Tauri/IPC. The caller
//! (`atlas-core`'s `AppFacade`) is responsible for assembling the source
//! text (existing chunks + Concept Graph data, per the task brief) and
//! actually invoking the model.

use serde::{Deserialize, Serialize};

/// Strips a leading/trailing markdown code fence if the model added one
/// despite being told not to -- same tolerance
/// `atlas_graph::extraction::strip_code_fence` applies, duplicated here
/// (rather than taking a dependency on `atlas-graph` for one helper) since
/// `atlas-models` sits below `atlas-graph` in the dependency graph.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
        return rest.trim();
    }
    trimmed
}

// ---------------------------------------------------------------------
// Mind Map: a node/edge graph, rendered by the frontend with the same
// lightweight graph-rendering approach `ConceptGraphView` (Phase 5)
// already established, per the task brief ("don't build a second
// graph-rendering component from scratch").
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MindMap {
    #[serde(default)]
    pub nodes: Vec<MindMapNode>,
    #[serde(default)]
    pub edges: Vec<MindMapEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MindMapNode {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MindMapEdge {
    pub from: String,
    pub to: String,
}

pub fn build_mind_map_prompt(source_label: &str, text: &str) -> String {
    format!(
        "You are building a mind map summarizing study material for an offline learning app. \
         Read the SOURCE TEXT below (from \"{source_label}\") and identify the central topics and \
         how they connect.\n\n\
         Respond with ONLY a single JSON object, no prose before or after it, no markdown code \
         fences, in exactly this shape:\n\
         {{\"nodes\": [{{\"id\": \"short-slug\", \"label\": \"Display label\"}}], \
         \"edges\": [{{\"from\": \"node id\", \"to\": \"node id\"}}]}}\n\n\
         Rules:\n\
         - Only include topics actually discussed in the text below -- never invent ones that \
         aren't there.\n\
         - Every \"from\"/\"to\" value in \"edges\" must exactly match an \"id\" in \"nodes\".\n\
         - Keep it to at most 20 nodes -- prefer the most central topics, not exhaustive detail.\n\
         - If nothing extractable is present, return {{\"nodes\": [], \"edges\": []}}.\n\n\
         SOURCE TEXT:\n{text}"
    )
}

pub fn parse_mind_map(raw: &str) -> Result<MindMap, serde_json::Error> {
    serde_json::from_str(strip_code_fence(raw))
}

// ---------------------------------------------------------------------
// Formula Sheet: a flat list of named formulas. `latex` is populated
// directly when the source text already contains recognizable notation;
// for formula regions that came from a scanned/image source, the caller
// is expected to have already run them through Vision-OCR transcription
// (existing `vision_ocr.rs` path, per the task brief) before this prompt
// ever sees them -- this module does not do OCR itself.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FormulaSheet {
    #[serde(default)]
    pub entries: Vec<FormulaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormulaEntry {
    pub name: String,
    pub latex: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn build_formula_sheet_prompt(source_label: &str, text: &str) -> String {
    format!(
        "You are compiling a formula sheet from study material for an offline learning app. Read \
         the SOURCE TEXT below (from \"{source_label}\") and extract every named formula or \
         equation it actually states, transcribing each as LaTeX.\n\n\
         Respond with ONLY a single JSON object, no prose before or after it, no markdown code \
         fences, in exactly this shape:\n\
         {{\"entries\": [{{\"name\": \"short name\", \"latex\": \"LaTeX source, no surrounding $ \
         delimiters\", \"description\": \"one sentence or null\"}}]}}\n\n\
         Rules:\n\
         - Only extract formulas actually present in the text below -- never invent ones that \
         aren't there.\n\
         - \"latex\" must be valid LaTeX math-mode source (no $ or \\\\[ \\\\] delimiters).\n\
         - If nothing extractable is present, return {{\"entries\": []}}.\n\n\
         SOURCE TEXT:\n{text}"
    )
}

pub fn parse_formula_sheet(raw: &str) -> Result<FormulaSheet, serde_json::Error> {
    serde_json::from_str(strip_code_fence(raw))
}

// ---------------------------------------------------------------------
// Study Guide: sections with a summary and key points.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StudyGuide {
    #[serde(default)]
    pub sections: Vec<StudyGuideSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudyGuideSection {
    pub heading: String,
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
}

pub fn build_study_guide_prompt(source_label: &str, text: &str) -> String {
    format!(
        "You are writing a study guide from material for an offline learning app. Read the SOURCE \
         TEXT below (from \"{source_label}\") and organize it into sections a student could \
         revise from.\n\n\
         Respond with ONLY a single JSON object, no prose before or after it, no markdown code \
         fences, in exactly this shape:\n\
         {{\"sections\": [{{\"heading\": \"string\", \"summary\": \"2-3 sentence summary\", \
         \"key_points\": [\"string\", ...]}}]}}\n\n\
         Rules:\n\
         - Base every section strictly on the text below -- never invent facts that aren't there.\n\
         - Prefer 3-8 sections; each with 2-6 key points.\n\
         - If nothing extractable is present, return {{\"sections\": []}}.\n\n\
         SOURCE TEXT:\n{text}"
    )
}

pub fn parse_study_guide(raw: &str) -> Result<StudyGuide, serde_json::Error> {
    serde_json::from_str(strip_code_fence(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mind_map_prompt_includes_source_label_and_text() {
        let prompt = build_mind_map_prompt("photosynthesis.pdf", "Plants convert light to energy.");
        assert!(prompt.contains("photosynthesis.pdf"));
        assert!(prompt.contains("Plants convert light to energy."));
    }

    #[test]
    fn parses_well_formed_mind_map_json() {
        let raw = r#"{"nodes":[{"id":"a","label":"Photosynthesis"}],"edges":[]}"#;
        let map = parse_mind_map(raw).unwrap();
        assert_eq!(map.nodes.len(), 1);
        assert_eq!(map.nodes[0].label, "Photosynthesis");
    }

    #[test]
    fn strips_a_markdown_code_fence_from_mind_map_response() {
        let raw = "```json\n{\"nodes\": [], \"edges\": []}\n```";
        let map = parse_mind_map(raw).unwrap();
        assert!(map.nodes.is_empty());
    }

    #[test]
    fn malformed_mind_map_json_is_an_error_not_a_panic() {
        assert!(parse_mind_map("not json").is_err());
    }

    #[test]
    fn parses_well_formed_formula_sheet_json() {
        let raw = r#"{"entries":[{"name":"Quadratic formula","latex":"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}","description":"Solves ax^2+bx+c=0"}]}"#;
        let sheet = parse_formula_sheet(raw).unwrap();
        assert_eq!(sheet.entries.len(), 1);
        assert_eq!(sheet.entries[0].name, "Quadratic formula");
    }

    #[test]
    fn empty_formula_sheet_response_parses_to_no_entries() {
        let sheet = parse_formula_sheet(r#"{"entries": []}"#).unwrap();
        assert!(sheet.entries.is_empty());
    }

    #[test]
    fn parses_well_formed_study_guide_json() {
        let raw = r#"{"sections":[{"heading":"Intro","summary":"Overview.","key_points":["a","b"]}]}"#;
        let guide = parse_study_guide(raw).unwrap();
        assert_eq!(guide.sections.len(), 1);
        assert_eq!(guide.sections[0].key_points.len(), 2);
    }

    #[test]
    fn malformed_study_guide_json_is_an_error_not_a_panic() {
        assert!(parse_study_guide("<html>not json</html>").is_err());
    }
}
