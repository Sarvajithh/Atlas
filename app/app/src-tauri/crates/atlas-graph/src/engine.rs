//! Graph Engine (§14, §20). Concept extraction (this milestone) runs as an
//! LLM-prompted step over newly-embedded chunks, deduplicates concept
//! names within a workspace, and persists nodes/edges via
//! [`GraphRepository`]. `atlas-graph` deliberately does not depend on
//! `atlas-models` (that would create a dependency cycle: `atlas-models`
//! already depends on `atlas-indexer`, and wiring the concrete inference
//! call lives above both, in `atlas-core`). Instead this module defines
//! the narrow [`ConceptExtractionModel`] seam it needs; `atlas-core` wires
//! a concrete adapter over `EnginePool`/`EngineRole::Reasoning` into it,
//! the same inversion `GraphRepository` already uses for storage.

use std::collections::HashMap;
use std::sync::Arc;

use atlas_events::EventBus;
use atlas_types::concept::{ConceptEdge, ConceptNode, RelationType};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;
use serde::Deserialize;

use crate::GraphRepository;

/// Seam for "ask a model for JSON text back", implemented in `atlas-core`
/// by an adapter over `atlas-models`' `EnginePool` (Reasoning role). Kept
/// this narrow (a single string-in/string-out method) so this crate's
/// extraction logic is unit-testable with a stub, without pulling in an
/// HTTP client, a Model Registry, or any Ollama-specific type.
pub trait ConceptExtractionModel: Send + Sync {
    fn extract(&self, prompt: &str) -> Result<String, AppError>;
}

/// One concept name + optional one-line description, as extracted from a
/// batch of chunk text.
#[derive(Debug, Clone, Deserialize)]
struct ExtractedConcept {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// One relation between two concept *names* (resolved to `ConceptNodeId`s
/// after dedup/insert, not before -- the model only ever sees text).
#[derive(Debug, Clone, Deserialize)]
struct ExtractedRelation {
    from: String,
    to: String,
    #[serde(rename = "type")]
    relation_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtractionResponse {
    #[serde(default)]
    concepts: Vec<ExtractedConcept>,
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

/// Result of running extraction over one document's worth of chunks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractionOutcome {
    pub nodes_created: usize,
    pub nodes_reused: usize,
    pub edges_created: usize,
}

fn normalize_label(label: &str) -> String {
    label.trim().to_lowercase()
}

fn relation_type_from_extracted(value: &str) -> RelationType {
    match value.trim().to_lowercase().as_str() {
        "prerequisite-of" | "prerequisite_of" | "prerequisiteof" => RelationType::PrerequisiteOf,
        "part-of" | "part_of" | "partof" => RelationType::PartOf,
        // "related-to" and anything unrecognized default to the most
        // general relation rather than dropping the edge -- an
        // imprecise-but-present edge is more useful for the graph view
        // than silently discarding a relation the model clearly intended.
        _ => RelationType::RelatedTo,
    }
}

/// Strips a leading/trailing markdown code fence (models frequently wrap
/// JSON in ```json ... ``` even when told not to) before parsing.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        let rest = rest.trim_start_matches('\n');
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
        return rest.trim();
    }
    trimmed
}

fn parse_extraction_response(raw: &str) -> Result<ExtractionResponse, serde_json::Error> {
    serde_json::from_str(strip_code_fence(raw))
}

/// Builds the structured-JSON extraction prompt for one batch of chunk
/// text (§40's "no Engine formats its own prompt" concerns the *inference*
/// role -- this is the Concept Graph's own domain-specific prompt shape,
/// analogous to how `atlas-models::prompt_builder` owns the RAG prompt
/// shape; kept here rather than in `atlas-models` because the JSON
/// contract it demands is entirely a Concept Graph concern).
pub fn build_extraction_prompt(document_label: &str, chunk_texts: &[String]) -> String {
    let joined = chunk_texts
        .iter()
        .enumerate()
        .map(|(idx, text)| format!("[chunk {}]\n{}", idx + 1, text))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "You are extracting a concept graph from study material.\n\n\
         Document: {document_label}\n\n\
         Read the text below and identify the distinct academic/technical \
         concepts it teaches, and the relationships between them.\n\n\
         Respond with ONLY a single JSON object, no prose before or after, \
         no markdown code fence, matching exactly this shape:\n\
         {{\"concepts\": [{{\"name\": \"string\", \"description\": \"one \
         sentence or null\"}}], \"relations\": [{{\"from\": \"concept \
         name\", \"to\": \"concept name\", \"type\": \"prerequisite-of\" | \
         \"related-to\" | \"part-of\"}}]}}\n\n\
         Rules:\n\
         - Use the same exact \"name\" spelling for a concept whenever it \
         recurs across relations.\n\
         - Only include relations between concepts you also listed.\n\
         - If no clear concepts are present, return {{\"concepts\": [], \
         \"relations\": []}}.\n\n\
         TEXT:\n\n{joined}"
    )
}

pub struct GraphEngine {
    repository: Arc<dyn GraphRepository>,
    events: Arc<dyn EventBus>,
    model: Option<Arc<dyn ConceptExtractionModel>>,
}

impl GraphEngine {
    pub fn new(repository: Arc<dyn GraphRepository>, events: Arc<dyn EventBus>) -> Self {
        Self {
            repository,
            events,
            model: None,
        }
    }

    /// Attaches the extraction model seam (§4: reused as an async
    /// post-embedding pipeline step, not at construction time in every
    /// caller that only needs read access to the graph -- e.g. the IPC
    /// query commands never need a model at all).
    pub fn with_model(mut self, model: Arc<dyn ConceptExtractionModel>) -> Self {
        self.model = Some(model);
        self
    }

    pub fn repository(&self) -> &Arc<dyn GraphRepository> {
        &self.repository
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    /// Extracts concepts/relations from one document's already-embedded
    /// chunks and merges them into the workspace's Concept Graph.
    /// - Dedup is by normalized (trimmed, lowercased) label *within the
    ///   workspace*, across documents, so re-indexing a second document
    ///   that mentions an already-known concept reuses its node rather
    ///   than creating a disconnected duplicate.
    /// - Idempotent-ish on re-run: existing edges with the same
    ///   (from, to, relation_type) are not re-inserted, so rebuilding a
    ///   workspace's graph does not grow edges unboundedly.
    pub fn extract_for_document(
        &self,
        workspace_id: WorkspaceId,
        document_label: &str,
        chunk_texts: &[String],
    ) -> Result<ExtractionOutcome, AppError> {
        let model = self.model.as_ref().ok_or_else(|| {
            AppError::model("GraphEngine::extract_for_document called without an extraction model attached")
        })?;

        if chunk_texts.is_empty() {
            return Ok(ExtractionOutcome::default());
        }

        let prompt = build_extraction_prompt(document_label, chunk_texts);

        // Retry-once-on-parse-failure: a model occasionally wraps its JSON
        // in prose despite instructions. One retry with an explicit
        // correction is enough signal without looping indefinitely against
        // a model that simply can't follow the format (§45.1 Recoverable:
        // the caller treats a persistent failure here as skippable, not
        // fatal to the rest of indexing -- see the pipeline hook).
        let raw = model.extract(&prompt)?;
        let parsed = match parse_extraction_response(&raw) {
            Ok(parsed) => parsed,
            Err(first_err) => {
                let retry_prompt = format!(
                    "{prompt}\n\nYour previous reply could not be parsed as JSON ({first_err}). \
                     Reply again with ONLY the JSON object, nothing else."
                );
                let retry_raw = model.extract(&retry_prompt)?;
                parse_extraction_response(&retry_raw).map_err(|second_err| {
                    AppError::model(format!(
                        "concept extraction response was not valid JSON after one retry: {second_err}"
                    ))
                })?
            }
        };

        self.merge_extraction(workspace_id, parsed)
    }

    fn merge_extraction(
        &self,
        workspace_id: WorkspaceId,
        parsed: ExtractionResponse,
    ) -> Result<ExtractionOutcome, AppError> {
        let mut outcome = ExtractionOutcome::default();

        // Load the workspace's existing nodes once so dedup is a normal
        // in-memory lookup rather than one repository round-trip per
        // extracted concept.
        let existing = self.repository.list_nodes_for_workspace(workspace_id)?;
        let mut by_normalized_label: HashMap<String, ConceptNode> = existing
            .into_iter()
            .map(|node| (normalize_label(&node.label), node))
            .collect();

        for concept in &parsed.concepts {
            let trimmed_name = concept.name.trim();
            if trimmed_name.is_empty() {
                continue;
            }
            let key = normalize_label(trimmed_name);
            if by_normalized_label.contains_key(&key) {
                outcome.nodes_reused += 1;
                continue;
            }

            let node = self.repository.insert_node(ConceptNode {
                id: ConceptNodeId(0),
                workspace_id,
                label: trimmed_name.to_string(),
                description: concept
                    .description
                    .as_ref()
                    .map(|d| d.trim().to_string())
                    .filter(|d| !d.is_empty()),
                created_at: now_iso8601(),
            })?;
            outcome.nodes_created += 1;
            by_normalized_label.insert(key, node);
        }

        for relation in &parsed.relations {
            let from_key = normalize_label(&relation.from);
            let to_key = normalize_label(&relation.to);
            if from_key == to_key {
                continue; // self-relations aren't meaningful in this graph
            }
            let (Some(from_node), Some(to_node)) =
                (by_normalized_label.get(&from_key), by_normalized_label.get(&to_key))
            else {
                // A relation naming a concept the model didn't also list
                // under "concepts" -- skip rather than inventing a node
                // for it from a bare relation endpoint (Rule in the
                // prompt asks the model not to do this; be defensive
                // anyway since it's an LLM output).
                continue;
            };

            let relation_type = relation_type_from_extracted(&relation.relation_type);
            let already_present = self
                .repository
                .list_edges_for_node(from_node.id)?
                .iter()
                .any(|e| {
                    e.from_node_id == from_node.id
                        && e.to_node_id == to_node.id
                        && e.relation_type == relation_type
                });
            if already_present {
                continue;
            }

            self.repository.insert_edge(ConceptEdge {
                id: ConceptEdgeId(0),
                from_node_id: from_node.id,
                to_node_id: to_node.id,
                relation_type,
                weight: 1.0,
            })?;
            outcome.edges_created += 1;
        }

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;
    use atlas_types::ids::WorkspaceId;
    use std::sync::Mutex;

    use crate::testing::InMemoryGraphRepository;

    #[test]
    fn engine_exposes_the_injected_dependencies() {
        let repository: Arc<dyn GraphRepository> = Arc::new(InMemoryGraphRepository::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let engine = GraphEngine::new(repository, events);

        assert!(engine
            .repository()
            .list_nodes_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
    }

    /// Stub model that returns a fixed sequence of responses, one per
    /// call -- lets tests exercise the retry-once path deterministically
    /// without a live Ollama instance.
    struct ScriptedModel {
        responses: Mutex<Vec<String>>,
    }

    impl ScriptedModel {
        fn new(responses: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into_iter().rev().map(String::from).collect()),
            })
        }
    }

    impl ConceptExtractionModel for ScriptedModel {
        fn extract(&self, _prompt: &str) -> Result<String, AppError> {
            self.responses
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| AppError::model("no more scripted responses"))
        }
    }

    fn engine_with(model: Arc<dyn ConceptExtractionModel>) -> GraphEngine {
        let repository: Arc<dyn GraphRepository> = Arc::new(InMemoryGraphRepository::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        GraphEngine::new(repository, events).with_model(model)
    }

    #[test]
    fn extract_for_document_without_a_model_attached_returns_a_model_error() {
        let repository: Arc<dyn GraphRepository> = Arc::new(InMemoryGraphRepository::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let engine = GraphEngine::new(repository, events);
        let err = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["some text".to_string()])
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn extract_for_document_with_empty_chunks_is_a_no_op() {
        let engine = engine_with(ScriptedModel::new(vec![]));
        let outcome = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &[])
            .unwrap();
        assert_eq!(outcome, ExtractionOutcome::default());
    }

    #[test]
    fn extract_for_document_parses_well_formed_json_and_persists_nodes_and_edges() {
        let response = r#"{"concepts":[{"name":"Derivatives","description":"rate of change"},
            {"name":"Gradient Descent","description":null}],
            "relations":[{"from":"Derivatives","to":"Gradient Descent","type":"prerequisite-of"}]}"#;
        let engine = engine_with(ScriptedModel::new(vec![response]));

        let outcome = engine
            .extract_for_document(WorkspaceId(1), "calc.pdf", &["chunk text".to_string()])
            .unwrap();

        assert_eq!(outcome.nodes_created, 2);
        assert_eq!(outcome.nodes_reused, 0);
        assert_eq!(outcome.edges_created, 1);

        let nodes = engine
            .repository()
            .list_nodes_for_workspace(WorkspaceId(1))
            .unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn extract_for_document_strips_a_markdown_code_fence() {
        let response = "```json\n{\"concepts\":[{\"name\":\"X\"}],\"relations\":[]}\n```";
        let engine = engine_with(ScriptedModel::new(vec![response]));
        let outcome = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        assert_eq!(outcome.nodes_created, 1);
    }

    #[test]
    fn extract_for_document_retries_once_on_unparseable_response_then_succeeds() {
        let engine = engine_with(ScriptedModel::new(vec![
            "not json at all",
            r#"{"concepts":[{"name":"X"}],"relations":[]}"#,
        ]));
        let outcome = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        assert_eq!(outcome.nodes_created, 1);
    }

    #[test]
    fn extract_for_document_gives_up_after_one_retry() {
        let engine = engine_with(ScriptedModel::new(vec!["still not json", "still not json either"]));
        let err = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn extract_for_document_dedupes_by_normalized_label_across_calls() {
        let engine = engine_with(ScriptedModel::new(vec![
            r#"{"concepts":[{"name":"Derivatives"}],"relations":[]}"#,
            r#"{"concepts":[{"name":"  derivatives "}],"relations":[]}"#,
        ]));
        engine
            .extract_for_document(WorkspaceId(1), "doc1.pdf", &["t".to_string()])
            .unwrap();
        let second = engine
            .extract_for_document(WorkspaceId(1), "doc2.pdf", &["t".to_string()])
            .unwrap();

        assert_eq!(second.nodes_created, 0);
        assert_eq!(second.nodes_reused, 1);
        assert_eq!(
            engine
                .repository()
                .list_nodes_for_workspace(WorkspaceId(1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn extract_for_document_skips_relations_naming_an_unlisted_concept() {
        let response = r#"{"concepts":[{"name":"A"}],"relations":[{"from":"A","to":"Ghost","type":"related-to"}]}"#;
        let engine = engine_with(ScriptedModel::new(vec![response]));
        let outcome = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        assert_eq!(outcome.edges_created, 0);
    }

    #[test]
    fn extract_for_document_is_idempotent_on_edges_when_rerun() {
        let response = r#"{"concepts":[{"name":"A"},{"name":"B"}],"relations":[{"from":"A","to":"B","type":"related-to"}]}"#;
        let engine = engine_with(ScriptedModel::new(vec![response, response]));
        engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        let second = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        assert_eq!(second.edges_created, 0, "re-running extraction must not duplicate edges");
    }

    #[test]
    fn unrecognized_relation_type_defaults_to_related_to_rather_than_dropping_the_edge() {
        let response = r#"{"concepts":[{"name":"A"},{"name":"B"}],"relations":[{"from":"A","to":"B","type":"something-weird"}]}"#;
        let engine = engine_with(ScriptedModel::new(vec![response]));
        let outcome = engine
            .extract_for_document(WorkspaceId(1), "doc.pdf", &["t".to_string()])
            .unwrap();
        assert_eq!(outcome.edges_created, 1);
    }
}
