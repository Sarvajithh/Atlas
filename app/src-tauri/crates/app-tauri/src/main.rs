//! app-tauri: the Tauri binary. Wires IPC commands to `atlas-core` (§11).
//! No business logic lives here (§26); this file only performs startup
//! (§41), registers commands (§12, §43), and launches the window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use atlas_core::startup::startup;

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
            commands::assistant::assistant_ask,
            commands::assistant::assistant_ask_stream,
            commands::assistant::assistant_cancel,
            commands::assistant::assistant_quiz,
            commands::assistant::assistant_flashcards,
            commands::assistant::assistant_revision_plan,
            commands::memory::memory_get_weaknesses,
            commands::graph::graph_get,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::rag::rag_search,
            commands::rag::rag_get_context,
            commands::ocr::ocr_reprocess,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Atlas");
}
