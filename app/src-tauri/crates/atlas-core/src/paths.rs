//! Shared "resolve a workspace-relative path to an absolute, safe-joined
//! filesystem path" helper (§21, §29). Extracted from `AppFacade` so both
//! the synchronous `index_document_now` IPC path and the
//! `IndexingWorker`'s background path use exactly one implementation
//! (task instruction: "Do NOT duplicate indexing logic").

use atlas_types::ids::WorkspaceId;
use atlas_utils::AppError;
use atlas_workspace::lifecycle::WorkspaceEngine;

pub(crate) fn resolve_absolute_path(
    workspace_engine: &WorkspaceEngine,
    workspace_id: WorkspaceId,
    relative_path: &str,
) -> Result<String, AppError> {
    let workspace = workspace_engine
        .get(workspace_id)?
        .ok_or_else(|| AppError::user(format!("workspace {workspace_id:?} not found")))?;
    let absolute_path = atlas_utils::paths::safe_join(
        std::path::Path::new(&workspace.root_path),
        relative_path,
    )
    .ok_or_else(|| AppError::user(format!("'{relative_path}' escapes the workspace root")))?;
    Ok(absolute_path.to_string_lossy().into_owned())
}
