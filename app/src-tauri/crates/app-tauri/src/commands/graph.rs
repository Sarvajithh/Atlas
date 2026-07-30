//! `graph.*` namespace (§43.1): graph.get, graph.getConceptDetail.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::concept::ConceptNode;
use atlas_types::ids::WorkspaceId;
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
