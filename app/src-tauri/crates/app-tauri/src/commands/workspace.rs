//! `workspace.*` namespace (§43.1): workspace.link, workspace.list,
//! workspace.archive.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::workspace::Workspace;
use atlas_utils::AppError;

#[tauri::command]
pub fn workspace_list(facade: State<'_, AppFacade>) -> Result<Vec<Workspace>, AppError> {
    facade.workspace_engine().repository().list()
}
