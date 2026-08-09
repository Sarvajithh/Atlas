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
    // §41: Startup Sequence. Database path is a placeholder here; the real
    // app-data-directory resolution is a future milestone (§23 Settings ->
    // storage locations).
    let facade = startup("atlas.sqlite3");

    tauri::Builder::default()
        .manage(facade)
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
