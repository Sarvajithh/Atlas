//! Structured error handling (§24, §45).
//!
//! All backend errors are structured (`AppError`), carrying a stable `code`
//! for programmatic handling and a human `message` for display (§24). Errors
//! are additionally categorized per §45.1 so every failure has a defined
//! handling path.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Error categories from §45.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Recoverable,
    Fatal,
    Retryable,
    UserError,
    SystemError,
    ModelError,
    WorkspaceError,
}

/// Error codes grouped by the categories in §24 (`FileSystemError`,
/// `IndexingError`, `EngineError`, `DbError`, `VectorDbError`,
/// `ValidationError`).
///
/// This list is frozen (§24, §32.4: "no architectural decision... changed...
/// without an explicit, separate instruction"). The broader error hierarchy
/// commonly requested by consumers (workspace / configuration / IPC /
/// parsing / model / storage / user errors) is expressed by *combining* a
/// `code` here with an `ErrorCategory` and, where useful, a `context`
/// string, rather than by adding new codes:
///
/// | Requested kind      | Expressed as                                          |
/// |----------------------|-------------------------------------------------------|
/// | Application error    | any `AppError` (this is the umbrella type)             |
/// | Workspace error       | `category = WorkspaceError` (§45.1)                    |
/// | Configuration error    | `code = ValidationError`, `category` per situation     |
/// | IPC error               | any `AppError` returned from a `#[tauri::command]`     |
/// |                          (§12: structured, never a raw string)         |
/// | Parsing error             | `code = IndexingError` (Parser Layer is part of the    |
/// |                            Indexing Module, §14, §36)                 |
/// | Indexing error             | `code = IndexingError`                                |
/// | Model error                  | `code = EngineError`, `category = ModelError`         |
/// | Storage error                  | `code = DbError` or `VectorDbError`                 |
/// | User error                       | `category = UserError`                            |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    FileSystemError,
    IndexingError,
    EngineError,
    DbError,
    VectorDbError,
    ValidationError,
}

/// The single structured error type used across every Atlas crate (§24, §45).
/// A bare, discarded error is a defect (§45.2); this type exists so every
/// failure carries enough context to be handled or surfaced honestly.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub category: ErrorCategory,
    pub context: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            category,
            context: None,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Workspace error (§45.1) -- e.g. root folder missing/unreadable.
    pub fn workspace(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::FileSystemError,
            ErrorCategory::WorkspaceError,
            message,
        )
    }

    /// Configuration/validation error (§23, Governing Principle).
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::ValidationError,
            ErrorCategory::UserError,
            message,
        )
    }

    /// Model/engine error (§24: "EngineError (e.g. Ollama unreachable)").
    pub fn model(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::EngineError, ErrorCategory::ModelError, message)
    }

    /// Storage error against the relational store (§33).
    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::DbError, ErrorCategory::SystemError, message)
    }

    /// Storage error against the vector store (§5, §33.4).
    pub fn vector_storage(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::VectorDbError,
            ErrorCategory::SystemError,
            message,
        )
    }

    /// Parsing/indexing error (§17, §36).
    pub fn indexing(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::IndexingError,
            ErrorCategory::Recoverable,
            message,
        )
    }

    /// User-caused error, surfaced directly and actionably (§45.1).
    pub fn user(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::ValidationError,
            ErrorCategory::UserError,
            message,
        )
    }

    /// A human-readable message suitable for direct display in the UI
    /// (§24: "a human `message` for display"), distinct from the more
    /// technical `Display` output used in logs.
    pub fn user_message(&self) -> String {
        match &self.context {
            Some(context) => format!("{} ({context})", self.message),
            None => self.message.clone(),
        }
    }
}

/// Conversion from filesystem I/O failures (§24: `FileSystemError`).
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::new(
            ErrorCode::FileSystemError,
            ErrorCategory::SystemError,
            err.to_string(),
        )
    }
}

/// Conversion from (de)serialization failures (§24: `ValidationError`).
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::new(
            ErrorCode::ValidationError,
            ErrorCategory::SystemError,
            err.to_string(),
        )
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_context_appends_context_to_user_message() {
        let err = AppError::user("missing folder").with_context("path: /tmp/x");
        assert_eq!(err.user_message(), "missing folder (path: /tmp/x)");
    }

    #[test]
    fn without_context_user_message_is_bare_message() {
        let err = AppError::workspace("root folder unreadable");
        assert_eq!(err.user_message(), "root folder unreadable");
    }

    #[test]
    fn helper_constructors_set_expected_code_and_category() {
        assert_eq!(
            AppError::workspace("x").category,
            ErrorCategory::WorkspaceError
        );
        assert_eq!(
            AppError::configuration("x").code,
            ErrorCode::ValidationError
        );
        assert_eq!(AppError::model("x").category, ErrorCategory::ModelError);
        assert_eq!(AppError::storage("x").code, ErrorCode::DbError);
        assert_eq!(AppError::vector_storage("x").code, ErrorCode::VectorDbError);
        assert_eq!(AppError::indexing("x").code, ErrorCode::IndexingError);
        assert_eq!(AppError::user("x").category, ErrorCategory::UserError);
    }

    #[test]
    fn io_error_converts_to_filesystem_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let app_err: AppError = io_err.into();
        assert_eq!(app_err.code, ErrorCode::FileSystemError);
    }

    #[test]
    fn json_error_converts_to_validation_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let app_err: AppError = json_err.into();
        assert_eq!(app_err.code, ErrorCode::ValidationError);
    }

    #[test]
    fn display_uses_code_and_message() {
        let err = AppError::new(ErrorCode::EngineError, ErrorCategory::Fatal, "boom");
        assert_eq!(err.to_string(), "EngineError: boom");
    }
}
