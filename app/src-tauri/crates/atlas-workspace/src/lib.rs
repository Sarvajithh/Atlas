//! atlas-workspace
//!
//! Workspace + folder link lifecycle (§6, §6.1). Defines the
//! `WorkspaceRepository` interface (Dependency Inversion, addendum
//! Governing Principle) and the `WorkspaceEngine` that depends on it.
//! Concrete storage is provided by atlas-db and wired in by atlas-core.
//!
//! This crate MUST NOT depend on atlas-db directly (forbidden edge:
//! "Workspace -> SQLite").

pub mod lifecycle;
pub mod repository;
pub mod testing;

pub use repository::WorkspaceRepository;
pub use testing::InMemoryWorkspaceRepository;
