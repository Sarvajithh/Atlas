//! app-tauri: the Tauri binary. Wires IPC commands to `atlas-core` (§11).
//! No business logic lives here (§26); this file only performs startup
//! (§41), registers commands (§12, §43), and launches the window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use atlas_core::shutdown::shutdown;
use atlas_core::startup::startup;
use atlas_core::AppFacade;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // §41: Startup Sequence, step 3 "Open Database". The database
            // must live in a stable, per-user, per-OS application data
            // directory (resolved via Tauri's `path()` API) rather than a
            // path relative to the process's current working directory --
            // a relative path silently pointed at a *different* (often
            // empty) SQLite file depending on how the app happened to be
            // launched (double-clicked bundle vs `tauri dev` vs a shortcut
            // with a different CWD), which meant every model selection
            // (§37, Model Dashboard) and other persisted state could look
            // like it "reset" on the next launch even though nothing was
            // actually lost -- the app was just reading a fresh, unrelated
            // database file. Resolving a real app-data directory up front
            // and creating it if it doesn't exist yet ensures the same
            // database file is opened on every launch.
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve the app data directory");
            std::fs::create_dir_all(&app_data_dir)
                .expect("failed to create the app data directory");
            let database_path = app_data_dir.join("atlas.sqlite3");
            atlas_utils::log_info!("[Startup] opening database at {}", database_path.display());

            let facade = startup(
                database_path
                    .to_str()
                    .expect("app data directory path was not valid UTF-8"),
            );
            app.manage(facade);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_list,
            commands::workspace::workspace_get,
            commands::workspace::workspace_link,
            commands::workspace::workspace_rename,
            commands::workspace::workspace_archive,
            commands::workspace::workspace_restore,
            commands::workspace::workspace_unlink,
            commands::workspace::workspace_indexing_status,
            commands::workspace::workspace_reindex,
            commands::assistant::assistant_ask,
            commands::assistant::assistant_ask_stream,
            commands::assistant::assistant_cancel,
            commands::assistant::assistant_quiz,
            commands::assistant::assistant_flashcards,
            commands::assistant::assistant_quiz_submit,
            commands::assistant::assistant_revision_plan,
            commands::assistant::assistant_list_sessions,
            commands::assistant::assistant_get_session_messages,
            commands::memory::memory_get_weaknesses,
            commands::graph::graph_get,
            commands::graph::graph_get_full,
            commands::graph::graph_reextract,
            commands::graph::graph_citation_graph,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::rag::rag_search,
            commands::rag::rag_get_context,
            commands::rag::rag_research_query,
            commands::search::search_global,
            commands::ocr::ocr_reprocess,
            commands::document::document_list,
            commands::document::document_get,
            commands::document::document_read,
            commands::bookmark::bookmark_list,
            commands::bookmark::bookmark_create,
            commands::bookmark::bookmark_delete,
            commands::model::model_list,
            commands::model::model_select,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Atlas")
        .run(|app_handle, event| {
            // §42: Shutdown Sequence, triggered on window close / app exit
            // (step 1: "Signal Shutdown Intent"). Runs the same `shutdown`
            // this crate already defined the ordered shape for -- this was
            // previously never invoked anywhere; wiring it in is this
            // task's "stop cleanly during shutdown" requirement for the
            // Background Indexing Worker.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let facade = app_handle.state::<AppFacade>();
                shutdown(&facade);
            }
        });
}
