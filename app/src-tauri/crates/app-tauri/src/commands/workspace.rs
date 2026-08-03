//! `workspace.*` namespace (§43.1): workspace.link, workspace.list,
//! workspace.rename, workspace.archive, workspace.restore,
//! workspace.unlink. Handlers only validate/forward/map errors (§26,
//! §46.4) -- all lifecycle logic lives in `atlas-core`'s `AppFacade` /
//! `atlas-workspace`'s `WorkspaceEngine`.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::WorkspaceId;
use atlas_types::job::IndexingStatus;
use atlas_types::workspace::Workspace;
use atlas_utils::AppError;

#[tauri::command]
pub fn workspace_list(facade: State<'_, AppFacade>) -> Result<Vec<Workspace>, AppError> {
    facade.workspace_engine().list()
}

#[tauri::command]
pub fn workspace_get(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Option<Workspace>, AppError> {
    facade.workspace_engine().get(WorkspaceId(workspace_id))
}

/// §6: "Users never upload files. They link folders." Links `root_path`,
/// performs the initial scan, and starts incremental watching (§21).
#[tauri::command]
pub fn workspace_link(
    facade: State<'_, AppFacade>,
    root_path: String,
    display_name: String,
) -> Result<Workspace, AppError> {
    facade.link_workspace(root_path, display_name)
}

#[tauri::command]
pub fn workspace_rename(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
    display_name: String,
) -> Result<Workspace, AppError> {
    facade
        .workspace_engine()
        .rename(WorkspaceId(workspace_id), display_name)
}

/// §6.1: "Archived: Watching stops. Derived data is retained and
/// queryable, but no new indexing happens."
#[tauri::command]
pub fn workspace_archive(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Workspace, AppError> {
    facade.archive_workspace(WorkspaceId(workspace_id))
}

#[tauri::command]
pub fn workspace_restore(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Workspace, AppError> {
    facade.restore_workspace(WorkspaceId(workspace_id))
}

/// §6.1: "Deleting a workspace link removes the workspace's row and
/// watcher registration; it does NOT delete AI Cache or Student Memory by
/// default."
#[tauri::command]
pub fn workspace_unlink(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<(), AppError> {
    facade.unlink_workspace(WorkspaceId(workspace_id))
}

/// Minimal backend state for a future Learning Progress UI (task scope):
/// queued/running/completed/failed job counts, the currently running
/// job's document, and a progress percentage, all read live from the
/// `jobs` table the Background Indexing Worker drives (`atlas-core`'s
/// `worker::compute_indexing_status`). No frontend is built against this
/// in this milestone.
#[tauri::command]
pub fn workspace_indexing_status(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<IndexingStatus, AppError> {
    facade.indexing_status(WorkspaceId(workspace_id))
}

/// "Rebuild Workspace Index" (re-walks the workspace and re-enqueues
/// every file for indexing). Returns the number of files enqueued; the
/// caller polls `workspace_indexing_status` for progress the same way it
/// already does for the initial scan -- no separate progress channel.
#[tauri::command]
pub fn workspace_reindex(facade: State<'_, AppFacade>, workspace_id: i64) -> Result<usize, AppError> {
    facade.reindex_workspace(WorkspaceId(workspace_id))
}
