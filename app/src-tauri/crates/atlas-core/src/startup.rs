//! Startup Sequence (§41). Each step MUST complete, or explicitly and
//! gracefully degrade, before the next begins (§41 closing note). Concrete
//! step bodies (migrations, model discovery, watcher start, etc.) are
//! deferred to later milestones; this defines the ordered shape.

use atlas_db::connection::SqliteConnection;
use atlas_utils::logging::init_logging;

use crate::facade::AppFacade;

/// Runs the startup sequence (§41) and returns the composed `AppFacade`.
pub fn startup(database_path: &str) -> AppFacade {
    // 1. Load Configuration -- deferred to atlas-config's provider once wired.
    // 2. Initialize Logging.
    init_logging();
    // 3. Open Database.
    let connection = SqliteConnection::open(database_path);
    // 4. Load Workspaces -- deferred (requires migrations, step 5+ later).
    // 5. Model Discovery -- deferred (requires an Ollama client, future milestone).
    // 6. Start Watchers -- deferred.
    // 7. Start Background Workers -- deferred.
    // 8. Initialize IPC -- performed by app-tauri after this call returns.
    // 9. Launch UI -- performed by app-tauri.
    // 10. Ready State -- signaled by app-tauri once IPC + UI are up.
    AppFacade::new(connection)
}
