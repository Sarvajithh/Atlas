//! `search.*` namespace (§9, §43.1): Global Search exposed to the UI.
//! Handlers only validate/forward/map errors (§26, §46.4) -- hybrid
//! retrieval, reranking, and cross-workspace merging all live in
//! `atlas-core`'s `AppFacade::search_global` (via `atlas-models`).

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::WorkspaceId;
use atlas_types::retrieval::GlobalSearchResult;
use atlas_utils::AppError;

/// Run Global Search (§9) for `query`, either scoped to `workspace_id` or,
/// when `workspace_id` is omitted, across every active workspace. Matches
/// the architecture doc's `search_global(query, scope: Workspace|All,
/// limit)` shape -- `scope` is expressed here as `workspace_id: Option<i64>`
/// (`Some` = Workspace, `None` = All) rather than a separate tagged enum,
/// since that's the minimum information the facade actually needs and
/// keeps the IPC argument shape simple or the frontend to construct.
#[tauri::command]
pub fn search_global(
    facade: State<'_, AppFacade>,
    query: String,
    workspace_id: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<GlobalSearchResult>, AppError> {
    facade.search_global(&query, workspace_id.map(WorkspaceId), limit)
}
