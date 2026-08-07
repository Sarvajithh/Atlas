//! `graph.*` namespace (§43.1): graph.get, graph.getEdges, graph.getConceptDetail.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::{ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;

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

/// Edges for every node in a workspace (§8.2.3 ConceptGraphView needs the
/// full edge set to draw a graph, not just the node list `graph_get`
/// already returns). Fetched node-by-node against `list_edges_for_node`
/// since `GraphRepository` has no bulk "edges for workspace" query (§33.5)
/// -- introducing one is additive-only and left for whichever milestone
/// first needs it at a scale where N+1 queries matter; workspace concept
/// counts here are small.
#[tauri::command]
pub fn graph_get_edges(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Vec<ConceptEdge>, AppError> {
    let repository = facade.graph_engine().repository();
    let nodes = repository.list_nodes_for_workspace(WorkspaceId(workspace_id))?;
    let mut edges = Vec::new();
    for node in nodes {
        edges.extend(repository.list_edges_for_node(node.id)?);
    }
    Ok(edges)
}

/// Single-node detail (§43.1 `graph.getConceptDetail`): the node plus its
/// outgoing edges, for a detail panel when the user selects a concept in
/// the graph view.
#[tauri::command]
pub fn graph_get_concept_detail(
    facade: State<'_, AppFacade>,
    node_id: i64,
) -> Result<Option<ConceptDetail>, AppError> {
    let repository = facade.graph_engine().repository();
    let Some(node) = repository.find_node(ConceptNodeId(node_id))? else {
        return Ok(None);
    };
    let edges = repository.list_edges_for_node(node.id)?;
    Ok(Some(ConceptDetail { node, edges }))
}

#[derive(serde::Serialize)]
pub struct ConceptDetail {
    pub node: ConceptNode,
    pub edges: Vec<ConceptEdge>,
}
