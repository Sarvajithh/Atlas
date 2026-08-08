//! Concept Graph construction/extraction (§20). This was previously the
//! documented gap: `GraphEngine` only held injected dependencies and
//! `engine.rs`'s own comment said extraction was "deferred to a future
//! milestone" -- no code path ever produced a node or edge from real
//! document content. This module closes that gap.
//!
//! Design mirrors the existing "feature built on an existing role"
//! pattern `atlas-models::engines` already uses for Quiz/Flashcard/
//! Revision Planner (§14.1's frozen Engine-role table is not extended):
//! extraction is a structured-output Reasoning-role call over already-
//! retrieved/parsed text, not a new Engine.
//!
//! `atlas-graph` must not depend on `atlas-models` directly (that would
//! cycle back through `atlas-models -> atlas-indexer`), so this defines
//! only a narrow [`ConceptExtractionModel`] seam; the concrete adapter
//! that wires it to `atlas-models::EnginePool` lives in `atlas-core`,
//! which is the one crate allowed to see both sides (composition root).

use std::sync::Arc;

use atlas_types::concept::{ConceptEdge, ConceptNode, RelationType};
use atlas_types::ids::WorkspaceId;
use atlas_utils::AppError;

use crate::repository::GraphRepository;

/// Narrow seam to whatever inference backend actually runs extraction.
/// Takes a fully-assembled prompt (built by [`build_extraction_prompt`])
/// and returns the raw model output. No Engine/model concerns leak into
/// this crate -- same "no Engine formats its own prompt" boundary
/// `atlas-models::prompt_builder` documents, just crossed the other
/// direction: this crate formats the prompt, the caller only runs it.
pub trait ConceptExtractionModel: Send + Sync {
    fn extract(&self, prompt: &str) -> Result<String, AppError>;
}

/// Result of running extraction over one document/chunk-batch: how many
/// concept nodes and cross-references were newly created versus already
/// existed (and so were reused rather than duplicated).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExtractionOutcome {
    pub nodes_created: usize,
    pub nodes_reused: usize,
    pub edges_created: usize,
    pub edges_skipped_existing: usize,
}

/// Shape the model is instructed to return. Kept intentionally small and
/// flat (label + optional description for nodes; from/to/type for edges)
/// so a Reasoning-role text model can produce it reliably without tool
/// calling.
#[derive(Debug, Clone, serde::Deserialize)]
struct ExtractedGraph {
    #[serde(default)]
    concepts: Vec<ExtractedConcept>,
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ExtractedConcept {
    label: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ExtractedRelation {
    from: String,
    to: String,
    #[serde(rename = "type")]
    relation_type: String,
}

/// Builds the Reasoning-role prompt for a batch of source text. Kept as a
/// free function (not a method with hidden state) so callers/tests can
/// inspect exactly what's sent, matching `prompt_builder.rs`'s own
/// "assembled, not hidden" style.
pub fn build_extraction_prompt(source_label: &str, text: &str) -> String {
    format!(
        "You are extracting a knowledge/concept graph from study material for an offline \
         learning app. Read the SOURCE TEXT below (from \"{source_label}\") and identify the \
         key concepts it discusses, plus any explicit relationships between them.\n\n\
         Respond with ONLY a single JSON object, no prose before or after it, no markdown code \
         fences, in exactly this shape:\n\
         {{\"concepts\": [{{\"label\": \"string\", \"description\": \"one sentence or null\"}}], \
         \"relations\": [{{\"from\": \"concept label\", \"to\": \"concept label\", \"type\": \
         \"prerequisite-of\" | \"related-to\" | \"part-of\"}}]}}\n\n\
         Rules:\n\
         - Only extract concepts that are actually discussed in the text below -- never invent \
         ones that aren't there.\n\
         - Every \"from\"/\"to\" value in \"relations\" must exactly match a \"label\" in \
         \"concepts\".\n\
         - \"type\" must be exactly one of prerequisite-of, related-to, part-of.\n\
         - If nothing extractable is present, return {{\"concepts\": [], \"relations\": []}}.\n\n\
         SOURCE TEXT:\n{text}"
    )
}

fn relation_type_from_extracted(value: &str) -> Option<RelationType> {
    match value {
        "prerequisite-of" => Some(RelationType::PrerequisiteOf),
        "related-to" => Some(RelationType::RelatedTo),
        "part-of" => Some(RelationType::PartOf),
        _ => None,
    }
}

/// Strips a leading/trailing markdown code fence if the model added one
/// despite being told not to (§45.1: tolerate a common, recoverable model
/// formatting slip rather than failing extraction outright).
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

/// Orchestrates extraction + storage. Construction happens during
/// indexing (§20's own doc comment: "Graph construction/updates happen
/// during indexing, not on every view render"), never on read.
pub struct ConceptExtractor {
    repository: Arc<dyn GraphRepository>,
    model: Arc<dyn ConceptExtractionModel>,
}

impl ConceptExtractor {
    pub fn new(repository: Arc<dyn GraphRepository>, model: Arc<dyn ConceptExtractionModel>) -> Self {
        Self { repository, model }
    }

    /// Extract concepts/relations from `text` (already-parsed/chunked
    /// document content -- this never re-parses a file itself, §36.3's
    /// module-boundary rule) and persist them for `workspace_id`,
    /// deduplicating against whatever the graph already has. `document_id`
    /// records provenance (Research Mode phase, §20: which document(s) a
    /// node was derived from), so the Citation Graph can later tell a
    /// genuinely cross-document relationship apart from a within-one-
    /// document one.
    ///
    /// A malformed or empty model response is a Recoverable error (§45.1):
    /// it's returned as `Err`, but the caller (the indexing worker) is
    /// expected to log and move on rather than fail the whole indexing
    /// job over an extraction miss -- indexing success and extraction
    /// success are independent outcomes.
    pub fn extract_and_store(
        &self,
        workspace_id: WorkspaceId,
        document_id: atlas_types::ids::DocumentId,
        source_label: &str,
        text: &str,
    ) -> Result<ExtractionOutcome, AppError> {
        if text.trim().is_empty() {
            return Ok(ExtractionOutcome::default());
        }

        let prompt = build_extraction_prompt(source_label, text);
        let raw = self.model.extract(&prompt)?;
        let cleaned = strip_code_fence(&raw);

        let parsed: ExtractedGraph = serde_json::from_str(cleaned).map_err(|e| {
            AppError::model(format!(
                "concept extraction model returned unparseable JSON: {e} (raw: {cleaned:.200})"
            ))
        })?;

        let mut outcome = ExtractionOutcome::default();
        let mut resolved: std::collections::HashMap<String, ConceptNode> =
            std::collections::HashMap::new();

        for concept in &parsed.concepts {
            let label = concept.label.trim();
            if label.is_empty() {
                continue;
            }
            if let Some(existing) = self.repository.find_node_by_label(workspace_id, label)? {
                outcome.nodes_reused += 1;
                self.repository.record_node_source(existing.id, document_id)?;
                resolved.insert(label.to_ascii_lowercase(), existing);
            } else {
                let created = self.repository.insert_node(ConceptNode {
                    id: atlas_types::ids::ConceptNodeId(0),
                    workspace_id,
                    label: label.to_string(),
                    description: concept.description.clone(),
                    created_at: atlas_utils::time::now_iso8601(),
                })?;
                outcome.nodes_created += 1;
                self.repository.record_node_source(created.id, document_id)?;
                resolved.insert(label.to_ascii_lowercase(), created);
            }
        }

        for relation in &parsed.relations {
            let (Some(from_node), Some(to_node)) = (
                resolved.get(&relation.from.trim().to_ascii_lowercase()),
                resolved.get(&relation.to.trim().to_ascii_lowercase()),
            ) else {
                // A relation referencing a label that wasn't in `concepts`
                // (model didn't follow the instruction) -- skip rather than
                // fabricate a node for it (§ "No mock/fabricated
                // cross-document relationships").
                continue;
            };
            if from_node.id == to_node.id {
                continue;
            }
            let Some(relation_type) = relation_type_from_extracted(&relation.relation_type) else {
                continue;
            };

            if self
                .repository
                .find_edge(from_node.id, to_node.id, &relation_type)?
                .is_some()
            {
                outcome.edges_skipped_existing += 1;
                continue;
            }

            self.repository.insert_edge(ConceptEdge {
                id: atlas_types::ids::ConceptEdgeId(0),
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
    use crate::testing::InMemoryGraphRepository;
    use atlas_types::ids::DocumentId;

    struct StubModel {
        response: String,
    }

    impl ConceptExtractionModel for StubModel {
        fn extract(&self, _prompt: &str) -> Result<String, AppError> {
            Ok(self.response.clone())
        }
    }

    fn extractor(response: &str) -> ConceptExtractor {
        ConceptExtractor::new(
            Arc::new(InMemoryGraphRepository::new()),
            Arc::new(StubModel { response: response.to_string() }),
        )
    }

    #[test]
    fn extracts_concepts_and_relations_from_well_formed_json() {
        let extractor = extractor(
            r#"{"concepts":[{"label":"Derivatives","description":"rate of change"},
                {"label":"Gradient Descent","description":null}],
               "relations":[{"from":"Derivatives","to":"Gradient Descent","type":"prerequisite-of"}]}"#,
        );

        let outcome = extractor
            .extract_and_store(WorkspaceId(1), DocumentId(1), "calc.pdf", "Derivatives measure rate of change...")
            .unwrap();

        assert_eq!(outcome.nodes_created, 2);
        assert_eq!(outcome.edges_created, 1);
        assert_eq!(outcome.nodes_reused, 0);
    }

    #[test]
    fn strips_a_markdown_code_fence_if_present() {
        let extractor = extractor(
            "```json\n{\"concepts\":[{\"label\":\"X\",\"description\":null}],\"relations\":[]}\n```",
        );
        let outcome = extractor
            .extract_and_store(WorkspaceId(1), DocumentId(1), "notes.md", "content about X")
            .unwrap();
        assert_eq!(outcome.nodes_created, 1);
    }

    #[test]
    fn reuses_an_existing_node_by_case_insensitive_label_instead_of_duplicating() {
        let repository = Arc::new(InMemoryGraphRepository::new());
        let existing = repository
            .insert_node(ConceptNode {
                id: atlas_types::ids::ConceptNodeId(0),
                workspace_id: WorkspaceId(1),
                label: "Gradient Descent".to_string(),
                description: None,
                created_at: "t0".to_string(),
            })
            .unwrap();
        repository.record_node_source(existing.id, DocumentId(1)).unwrap();
        let extractor = ConceptExtractor::new(
            repository.clone(),
            Arc::new(StubModel {
                response: r#"{"concepts":[{"label":"gradient descent","description":null}],"relations":[]}"#
                    .to_string(),
            }),
        );

        let outcome = extractor
            .extract_and_store(WorkspaceId(1), DocumentId(2), "notes2.md", "more about gradient descent")
            .unwrap();

        assert_eq!(outcome.nodes_created, 0);
        assert_eq!(outcome.nodes_reused, 1);
        assert_eq!(repository.list_nodes_for_workspace(WorkspaceId(1)).unwrap().len(), 1);

        // Provenance (Research Mode phase): the pre-existing node (already
        // sourced from whatever document created it originally) now also
        // records this second document as a source -- the Citation Graph
        // needs both to tell this apart from a within-one-document
        // relationship.
        let node_id = repository.list_nodes_for_workspace(WorkspaceId(1)).unwrap()[0].id;
        let mut sources = repository.list_source_documents(node_id).unwrap();
        sources.sort_by_key(|d| d.0);
        assert_eq!(sources, vec![DocumentId(1), DocumentId(2)]);
    }

    #[test]
    fn skips_a_relation_whose_endpoint_was_not_in_concepts() {
        let extractor = extractor(
            r#"{"concepts":[{"label":"X","description":null}],
               "relations":[{"from":"X","to":"Y","type":"related-to"}]}"#,
        );
        let outcome = extractor.extract_and_store(WorkspaceId(1), DocumentId(1), "s", "text").unwrap();
        assert_eq!(outcome.nodes_created, 1);
        assert_eq!(outcome.edges_created, 0);
    }

    #[test]
    fn does_not_insert_a_duplicate_edge_on_re_extraction() {
        let repository = Arc::new(InMemoryGraphRepository::new());
        let response = r#"{"concepts":[{"label":"A","description":null},{"label":"B","description":null}],
               "relations":[{"from":"A","to":"B","type":"related-to"}]}"#;
        let extractor = ConceptExtractor::new(
            repository.clone(),
            Arc::new(StubModel { response: response.to_string() }),
        );

        extractor.extract_and_store(WorkspaceId(1), DocumentId(1), "s", "text").unwrap();
        let second = extractor.extract_and_store(WorkspaceId(1), DocumentId(1), "s", "text").unwrap();

        assert_eq!(second.nodes_reused, 2);
        assert_eq!(second.edges_created, 0);
        assert_eq!(second.edges_skipped_existing, 1);
    }

    #[test]
    fn empty_text_is_a_no_op_and_never_calls_the_model() {
        struct PanicModel;
        impl ConceptExtractionModel for PanicModel {
            fn extract(&self, _prompt: &str) -> Result<String, AppError> {
                panic!("should not be called for empty text");
            }
        }
        let extractor = ConceptExtractor::new(
            Arc::new(InMemoryGraphRepository::new()),
            Arc::new(PanicModel),
        );
        let outcome = extractor.extract_and_store(WorkspaceId(1), DocumentId(1), "s", "   ").unwrap();
        assert_eq!(outcome, ExtractionOutcome::default());
    }

    #[test]
    fn malformed_json_is_a_recoverable_error_not_a_panic() {
        let extractor = extractor("this is not json");
        let err = extractor.extract_and_store(WorkspaceId(1), DocumentId(1), "s", "text").unwrap_err();
        assert!(err.message.contains("unparseable"));
    }

    #[test]
    fn build_extraction_prompt_includes_source_label_and_text() {
        let prompt = build_extraction_prompt("calc.pdf", "Derivatives are...");
        assert!(prompt.contains("calc.pdf"));
        assert!(prompt.contains("Derivatives are..."));
    }
}
