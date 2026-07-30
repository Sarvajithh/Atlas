//! `assistant.*` namespace (§43.1): assistant.ask, assistant.cancel.
//! Full Scheduler wiring (§15) is a future milestone; these handlers define
//! the command shape only.

use atlas_utils::error::{ErrorCategory, ErrorCode};
use atlas_utils::AppError;

fn not_yet_implemented(command: &str) -> AppError {
    AppError::new(
        ErrorCode::EngineError,
        ErrorCategory::Recoverable,
        format!("{command} is not implemented in this milestone"),
    )
}

#[tauri::command]
pub fn assistant_ask(_question: String) -> Result<String, AppError> {
    Err(not_yet_implemented("assistant.ask"))
}

#[tauri::command]
pub fn assistant_cancel(_request_id: String) -> Result<(), AppError> {
    Err(not_yet_implemented("assistant.cancel"))
}
