//! atlas-graph
//!
//! Concept Graph domain logic (§20). Graph construction/updates happen
//! during indexing, not on every view render; the Concept Graph View reads
//! this data read-only (§20). Defines the repository interface only;
//! concrete SQLite storage is provided by atlas-db.

pub mod engine;
pub mod repository;
pub mod testing;

pub use engine::GraphEngine;
pub use repository::GraphRepository;
pub use testing::InMemoryGraphRepository;
