//! Startup Sequence (§41). Each step MUST complete, or explicitly and
//! gracefully degrade, before the next begins (§41 closing note). Concrete
//! step bodies (migrations, model discovery, watcher start, etc.) are
//! deferred to later milestones; this defines the ordered shape.

use atlas_db::connection::SqliteConnection;
use atlas_utils::{log_warn, logging::init_logging};

use crate::facade::AppFacade;

/// Runs the startup sequence (§41) and returns the composed `AppFacade`.
pub fn startup(database_path: &str) -> AppFacade {
    // 1. Load Configuration -- deferred to atlas-config's provider once wired.
    // 2. Initialize Logging.
    init_logging();
    // 3. Open Database (also runs pending migrations, §41 step 3).
    let connection = SqliteConnection::open(database_path);
    // 4. Load Workspaces -- happens implicitly: AppFacade::new wires the
    //    WorkspaceEngine directly over the now-migrated `workspaces` table;
    //    nothing needs to be eagerly materialized into memory (§9's
    //    Workspace Engine facade queries on demand).
    let facade = AppFacade::new(connection);
    // 5. Model Discovery -- deferred (requires an Ollama client, future milestone).
    // 6/7. Start Watchers + resume in-flight Background Workers (§41 steps
    //    6-7, §21 "Active: steady state"). A watcher failing to start for
    //    one workspace must not abort startup for the rest of the app
    //    (§41 closing note: "gracefully degrade") -- errors are logged,
    //    not propagated.
    if let Err(err) = facade.resume_watchers() {
        log_warn!("failed to resume one or more folder watchers on startup: {err}");
    }
    // 8. Initialize IPC -- performed by app-tauri after this call returns.
    // 9. Launch UI -- performed by app-tauri.
    // 10. Ready State -- signaled by app-tauri once IPC + UI are up.
    facade
}
