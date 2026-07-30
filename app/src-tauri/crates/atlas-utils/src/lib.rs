//! atlas-utils
//!
//! Cross-cutting utilities shared by every crate: the structured error type
//! (§24, §45), the logging bootstrap (§41), and reusable, domain-agnostic
//! helpers (paths, filesystem, hashing, time, serialization, validation).
//! Contains no domain logic -- nothing here knows about workspaces,
//! documents, engines, or models.

pub mod error;
pub mod fs;
pub mod hashing;
pub mod logging;
pub mod paths;
pub mod serialization;
pub mod time;
pub mod validation;

pub use error::{AppError, ErrorCategory, ErrorCode};
pub use logging::{LogLevel, Logger};
