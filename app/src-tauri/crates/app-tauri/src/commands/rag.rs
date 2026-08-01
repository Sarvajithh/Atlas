//! `rag.*` namespace (§43.1): the Knowledge Engine's read path (§18, §39,
//! §40) exposed to the UI. Handlers only validate/forward/map errors (§26,
//! §46.4) -- hybrid retrieval, reranking, and context/prompt assembly all
//! live in `atlas-core`'s `AppFacade` (via `atlas-models`).

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::WorkspaceId;
use atlas_types::retrieval::Citation;
use atlas_utils::AppError;
use serde::Serialize;

/// Response shape for `rag.search`/`rag.getContext`: the assembled prompt
/// content plus the citations it carries (§44.1), so the UI can render
/// clickable citation markers without a second round-trip.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub content: String,
    pub citations: Vec<Citation>,
}

/// Run hybrid retrieval + reranking + context assembly for `query` within
/// `workspace_id` (§18), returning the resulting content and citations.
#[tauri::command]
pub fn rag_search(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
    query: String,
    limit: Option<usize>,
) -> Result<SearchResult, AppError> {
    let (content, citations) = facade.search(WorkspaceId(workspace_id), &query, limit.unwrap_or(10))?;
    Ok(SearchResult { content, citations })
}

/// Alias for `rag.search` under the name the architecture doc's §43.1
/// command table uses for "build context for a query" -- kept as a
/// distinct command (rather than only `rag_search`) since a future Tutor
/// Engine milestone may want to call context assembly without needing the
/// final prompt string, at which point this handler's body diverges from
/// `rag_search`'s.
#[tauri::command]
pub fn rag_get_context(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
    query: String,
    limit: Option<usize>,
) -> Result<SearchResult, AppError> {
    rag_search(facade, workspace_id, query, limit)
}
