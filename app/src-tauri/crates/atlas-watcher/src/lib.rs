//! atlas-watcher
//!
//! Folder Watcher (§21): OS-level filesystem watching per linked workspace
//! root, debouncing, and event -> indexing job translation. Emits
//! `FileAdded` / `FileUpdated` / `FileDeleted` (§34.2) through the Event Bus
//! rather than calling the Indexing module directly (§46.6).

pub mod debounce;
pub mod scan;
pub mod watcher;

pub use debounce::{DebouncedChangeKind, Debouncer, RawChange, RawChangeKind};
pub use scan::scan_files;
pub use watcher::FolderWatcher;
