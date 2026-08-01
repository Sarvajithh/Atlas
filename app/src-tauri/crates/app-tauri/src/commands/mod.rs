//! IPC command handlers (§12, §43). Grouped by domain namespace (§43.1).
//! Handlers validate input, call into `atlas-core`, map errors, and return
//! -- no business logic lives here (§26, §46.4).

pub mod workspace;
pub mod assistant;
pub mod memory;
pub mod graph;
pub mod settings;
pub mod rag;
pub mod ocr;
