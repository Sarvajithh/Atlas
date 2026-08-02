//! `document.*` namespace (§43.1, new for the Document Experience
//! milestone): expose the already-implemented `DocumentRepository`
//! (`atlas-indexer`, backed by `SqliteDocumentRepository` in atlas-db) and
//! serve raw file bytes for viewers. Handlers only validate/forward/map
//! errors (§26, §46.4) -- no parsing/indexing logic is duplicated here;
//! `document_read` reuses the same `safe_join` path-arithmetic helper
//! `IndexingPipeline::index_document`'s caller already uses (§29:
//! read-only, sandboxed access to Source Documents).

use base64::Engine;
use tauri::State;

use atlas_core::AppFacade;
use atlas_types::document::DocumentRecord;
use atlas_types::ids::{DocumentId, WorkspaceId};
use atlas_utils::paths::safe_join;
use atlas_utils::AppError;
use serde::Serialize;

#[tauri::command]
pub fn document_list(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
) -> Result<Vec<DocumentRecord>, AppError> {
    facade
        .indexing_pipeline()
        .documents()
        .list_for_workspace(WorkspaceId(workspace_id))
}

#[tauri::command]
pub fn document_get(
    facade: State<'_, AppFacade>,
    document_id: i64,
) -> Result<Option<DocumentRecord>, AppError> {
    facade.indexing_pipeline().documents().find_by_id(DocumentId(document_id))
}

/// A file's bytes for the Document Viewer (§8.2.2-§8.2.4 -- PDF/Markdown/
/// Image viewers). Text-like types (`md`) are returned as UTF-8 text;
/// binary types (`pdf`, `image`) are base64-encoded, matching how they'd
/// need to cross the IPC boundary as JSON either way.
#[derive(Debug, Serialize)]
pub struct DocumentContent {
    pub relative_path: String,
    pub file_type: String,
    pub mime: String,
    pub is_base64: bool,
    pub content: String,
}

fn mime_for(file_type: &str, relative_path: &str) -> &'static str {
    match file_type {
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "image" => {
            let lower = relative_path.to_lowercase();
            if lower.ends_with(".png") {
                "image/png"
            } else if lower.ends_with(".gif") {
                "image/gif"
            } else if lower.ends_with(".webp") {
                "image/webp"
            } else if lower.ends_with(".bmp") {
                "image/bmp"
            } else {
                "image/jpeg"
            }
        }
        _ => "application/octet-stream",
    }
}

#[tauri::command]
pub fn document_read(
    facade: State<'_, AppFacade>,
    document_id: i64,
) -> Result<DocumentContent, AppError> {
    let document = facade
        .indexing_pipeline()
        .documents()
        .find_by_id(DocumentId(document_id))?
        .ok_or_else(|| AppError::user(format!("document {document_id} not found")))?;

    let workspace = facade
        .workspace_engine()
        .get(document.workspace_id)?
        .ok_or_else(|| AppError::workspace(format!("workspace {} not found", document.workspace_id.0)))?;

    let root = std::path::Path::new(&workspace.root_path);
    let absolute_path = safe_join(root, &document.relative_path)
        .ok_or_else(|| AppError::user("document path escapes workspace root"))?;

    let bytes = std::fs::read(&absolute_path)
        .map_err(|e| AppError::indexing(format!("failed to read '{}': {e}", absolute_path.display())))?;

    let mime = mime_for(&document.file_type, &document.relative_path);
    let is_base64 = document.file_type != "md";
    let content = if is_base64 {
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    } else {
        String::from_utf8(bytes)
            .map_err(|e| AppError::indexing(format!("document is not valid UTF-8: {e}")))?
    };

    Ok(DocumentContent {
        relative_path: document.relative_path,
        file_type: document.file_type,
        mime: mime.to_string(),
        is_base64,
        content,
    })
}
