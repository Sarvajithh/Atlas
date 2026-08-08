//! atlas-types
//!
//! Shared, serializable domain types used across every Atlas crate.
//! This crate defines *shapes only* (structs/enums) with no business logic,
//! no I/O, and no dependency on any other `atlas-*` crate (§11).
//!
//! Cross-crate contracts described in the architecture contract (§11, §33)
//! live here so that domain crates can exchange data without depending on
//! each other's internals.

pub mod chat;
pub mod chunk;
pub mod concept;
pub mod document;
pub mod event;
pub mod ids;
pub mod job;
pub mod memory;
pub mod model;
pub mod quiz;
pub mod retrieval;
pub mod settings;
pub mod workspace;

// Re-export the most commonly used types at the crate root for convenience.
pub use ids::*;
