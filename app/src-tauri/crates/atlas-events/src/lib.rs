//! atlas-events
//!
//! The in-process Event Bus (§34). Decouples modules so that engines and
//! background systems never call each other directly when an event
//! relationship is more appropriate (§34, §46.6).
//!
//! This crate defines the `EventBus` interface and the `EventSubscriber`
//! trait, plus a dependency-free [`InMemoryEventBus`] implementation used
//! both as a lightweight default and as this crate's own testing double
//! (§30). The durable, SQLite-backed log (`events` table, §33.15) is
//! implemented by atlas-db and injected at composition time by atlas-core
//! (Governing Principle: Dependency Inversion everywhere).

pub mod bus;
pub mod in_memory;

pub use bus::{EventBus, EventSubscriber};
pub use in_memory::InMemoryEventBus;
