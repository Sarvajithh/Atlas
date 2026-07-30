//! Shutdown Sequence (§42). No step may be skipped to speed up shutdown
//! (§42 closing note). Concrete step bodies are deferred to later
//! milestones; this defines the ordered shape.

use crate::facade::AppFacade;

/// Runs the shutdown sequence (§42) against a composed `AppFacade`.
pub fn shutdown(_facade: &AppFacade) {
    // 1. Signal Shutdown Intent -- triggered by caller (app-tauri window close).
    // 2. Stop accepting new Jobs -- deferred (Background Job System, §36).
    // 3. Drain/Cancel in-flight Jobs -- deferred.
    // 4. Stop Watchers -- deferred.
    // 5. Flush Caches -- deferred.
    // 6. Close Database -- deferred (connection lifetime managed by SqliteConnection).
    // 7. Flush Logs -- deferred (logging backend not yet selected).
    // 8. Release Resources -- deferred (Resource Manager, §38).
}
