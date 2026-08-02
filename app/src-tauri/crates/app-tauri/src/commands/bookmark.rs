//! `bookmark.*` namespace (§43.1, new for the Document Experience
//! milestone): thin passthrough to the already-implemented
//! `BookmarkRepository` (§33.9), reached via `AppFacade::memory_engine`.
//! Handlers only validate/forward/map errors (§26, §46.4).

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::{BookmarkId, DocumentId};
use atlas_types::memory::Bookmark;
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;

#[tauri::command]
pub fn bookmark_list(
    facade: State<'_, AppFacade>,
    document_id: i64,
) -> Result<Vec<Bookmark>, AppError> {
    facade
        .memory_engine()
        .bookmarks()
        .list_for_document(DocumentId(document_id))
}

#[tauri::command]
pub fn bookmark_create(
    facade: State<'_, AppFacade>,
    document_id: i64,
    location_ref: String,
    label: String,
) -> Result<Bookmark, AppError> {
    facade.memory_engine().bookmarks().insert(Bookmark {
        id: BookmarkId(0),
        document_id: DocumentId(document_id),
        location_ref,
        label,
        created_at: now_iso8601(),
    })
}

#[tauri::command]
pub fn bookmark_delete(
    facade: State<'_, AppFacade>,
    bookmark_id: i64,
) -> Result<(), AppError> {
    facade.memory_engine().bookmarks().delete(BookmarkId(bookmark_id))
}
