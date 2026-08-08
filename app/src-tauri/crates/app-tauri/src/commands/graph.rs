//! `graph.*` namespace (§43.1): graph.get, graph.getFull, graph.reextract,
//! graph.getConceptDetail, graph.citationGraph (Research Mode phase).

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::WorkspaceId;
use atlas_utils::AppError;
use serde::Serialize;

#[tauri::command]
pub fn graph_get(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Vec<ConceptNode>, AppError> {
    facade
        .graph_engine()
        .repository()
        .list_nodes_for_workspace(WorkspaceId(workspace_id))
}

/// Full graph (nodes + edges) for node-link rendering (§20). See
/// `AppFacade::graph_full`'s doc comment for why `graph_get` alone was
/// never enough to draw an actual graph.
#[derive(Debug, Serialize)]
pub struct GraphFullResponse {
    pub nodes: Vec<ConceptNode>,
    pub edges: Vec<ConceptEdge>,
}

#[tauri::command]
pub fn graph_get_full(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<GraphFullResponse, AppError> {
    let (nodes, edges) = facade.graph_full(WorkspaceId(workspace_id))?;
    Ok(GraphFullResponse { nodes, edges })
}

/// Manual Concept Extraction re-run for a workspace (§20). See
/// `AppFacade::reextract_workspace_concepts`'s doc comment for the
/// idempotency/error-handling contract.
#[tauri::command]
pub fn graph_reextract(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<atlas_graph::ExtractionOutcome, AppError> {
    facade.reextract_workspace_concepts(WorkspaceId(workspace_id))
}

/// One entry in `graph.citationGraph`'s response: a real Concept Graph
/// edge whose endpoints are, between them, sourced from more than one
/// document -- the wire shape `CitationGraphView.tsx` renders directly,
/// resolved from `atlas_graph::CrossDocumentEdge` (which carries full
/// `ConceptNode`/`ConceptEdge` structs the frontend doesn't need in full).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationGraphEdge {
    pub edge: ConceptEdge,
    pub from_label: String,
    pub to_label: String,
    pub source_document_ids: Vec<i64>,
}

/// Research Mode's Citation Graph (§ objective "citation graph /
/// cross-document linking"): every Concept Graph edge across the given
/// workspaces that spans more than one document's real extracted
/// provenance. `workspace_ids` empty is treated as "no workspaces
/// selected" (an empty result), not "all workspaces" -- the frontend is
/// expected to pass the caller's actual selection explicitly, same as
/// `rag.researchQuery`.
#[tauri::command]
pub fn graph_citation_graph(
    facade: State<'_, AppFacade>,
    workspace_ids: Vec<i64>,
) -> Result<Vec<CitationGraphEdge>, AppError> {
    let workspace_ids: Vec<WorkspaceId> = workspace_ids.into_iter().map(WorkspaceId).collect();
    let cross_document_edges = facade.citation_graph(&workspace_ids)?;
    Ok(cross_document_edges
        .into_iter()
        .map(|e| CitationGraphEdge {
            edge: e.edge,
            from_label: e.from_node.label,
            to_label: e.to_node.label,
            source_document_ids: e.source_documents.into_iter().map(|d| d.0).collect(),
        })
        .collect())
}
