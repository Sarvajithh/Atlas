//! Shutdown Sequence (§42). No step may be skipped to speed up shutdown
//! (§42 closing note). Concrete step bodies are deferred to later
//! milestones; this defines the ordered shape.

use crate::facade::AppFacade;

/// Runs the shutdown sequence (§42) against a composed `AppFacade`.
pub fn shutdown(facade: &AppFacade) {
    // 1. Signal Shutdown Intent -- triggered by caller (app-tauri window close).
    // 2. Stop accepting new Jobs / 3. Drain/Cancel in-flight Jobs: the
    // Background Indexing Worker's `stop` signals its poll loop to exit
    // (so no *new* job is claimed) and joins the worker thread, letting
    // whatever job is already in flight finish via the synchronous
    // `IndexingPipeline::index_document` call (§21, §42 closing note: "no
    // step may be skipped to speed up shutdown").
    if let Err(err) = facade.stop_indexing_worker() {
        atlas_utils::log_warn!("failed to stop the background indexing worker cleanly: {err}");
    }
    // 4. Stop Watchers -- deferred.
    // 5. Flush Caches -- deferred.
    // 6. Close Database -- deferred (connection lifetime managed by SqliteConnection).
    // 7. Flush Logs -- deferred (logging backend not yet selected).
    // 8. Release Resources -- deferred (Resource Manager, §38).
}
