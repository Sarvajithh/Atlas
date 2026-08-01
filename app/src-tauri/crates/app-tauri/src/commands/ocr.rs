//! `ocr.*` / indexing namespace (§43.1): expose the Knowledge Engine's
//! indexing pipeline (§17, §18, §36) for a single file, on demand -- e.g.
//! "reprocess this file now" after the user notices a bad OCR result or
//! edits a file outside of the Folder Watcher's view. Handlers only
//! validate/forward/map errors (§26, §46.4); the pipeline itself lives in
//! `atlas-indexer`, wired through `atlas-core`'s `AppFacade`.

use tauri::State;

use atlas_core::AppFacade;
use atlas_indexer::pipeline::IndexOutcome;
use atlas_types::ids::WorkspaceId;
use atlas_utils::AppError;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum OcrReprocessResult {
    Skipped,
    Indexed { chunk_count: usize },
}

impl From<IndexOutcome> for OcrReprocessResult {
    fn from(outcome: IndexOutcome) -> Self {
        match outcome {
            IndexOutcome::Skipped => OcrReprocessResult::Skipped,
            IndexOutcome::Indexed { chunk_count } => OcrReprocessResult::Indexed { chunk_count },
        }
    }
}

/// Re-run the full parse -> OCR -> chunk -> embed pipeline for one file
/// right now, bypassing the usual content-hash skip only in the sense
/// that a changed file is always re-processed (§22) -- an unchanged file
/// still reports `Skipped` rather than doing needless work.
#[tauri::command]
pub fn ocr_reprocess(
    facade: State<'_, AppFacade>,
    workspace_id: i64,
    relative_path: String,
) -> Result<OcrReprocessResult, AppError> {
    let outcome = facade.index_document_now(WorkspaceId(workspace_id), &relative_path)?;
    Ok(outcome.into())
}
