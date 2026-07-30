//! atlas-db
//!
//! SQLite schema + queries (§10.1, §11, §33). Implements every repository
//! interface defined by the domain crates against a single SQLite
//! connection (§5, §7). This is the only crate that touches SQLite
//! directly; everything else depends on the interfaces (Governing
//! Principle).
//!
//! Connection/migration setup is deferred to the database-implementation
//! milestone (see Cargo.toml note); each adapter below defines the correct
//! shape and trait wiring now so the workspace compiles end-to-end.

pub mod chunk_adapter;
pub mod connection;
pub mod document_adapter;
pub mod event_bus_adapter;
pub mod graph_adapter;
pub mod memory_adapter;
pub mod model_registry_adapter;
pub mod settings_adapter;
pub mod workspace_adapter;

pub use connection::SqliteConnection;
