//! atlas-core
//!
//! The composition root. Wires concrete infrastructure (atlas-db,
//! atlas-vector) into the interfaces domain crates (atlas-workspace,
//! atlas-indexer, atlas-models, atlas-memory, atlas-graph) depend on, and
//! exposes a single `AppFacade` to app-tauri. This is the one place the
//! full dependency graph is visible at once (Dependency Inversion,
//! Governing Principle).
//!
//! **Dependency Injection**: `AppFacade::new` *is* the application's DI
//! container. Every service is constructed once here, wrapped in `Arc`
//! (Singleton lifetime -- the only lifetime any Atlas service needs, since
//! there is exactly one of each per running app instance, §2.1/§6), and
//! handed to whichever higher-level engine depends on its trait. A second,
//! generic DI framework is not introduced alongside this: it would
//! duplicate the exact responsibility this composition root already has
//! (§46.2), and constructor injection through plain `Arc<dyn Trait>` is
//! precisely what the Governing Principle's "Dependency Inversion
//! everywhere" already prescribes. Mock/test support (§30) is achieved the
//! same way: substitute any in-memory test double from a crate's
//! `testing` module for the `Sqlite*` adapter at construction time -- see
//! `facade::tests` for an example wiring a fully mocked `AppFacade`-shaped
//! set of engines with zero SQLite involved.
//!
//! Also owns the Startup Sequence (§41) and Shutdown Sequence (§42)
//! skeletons, and the transient Application State (§13, see [`state`]).

pub mod facade;
pub mod shutdown;
pub mod startup;
pub mod state;

pub use facade::AppFacade;
pub use state::AppState;
