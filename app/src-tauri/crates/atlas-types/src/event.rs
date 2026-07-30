//! Durable event log shapes (§34, §33.15).

use serde::{Deserialize, Serialize};

use crate::ids::EventId;

/// Canonical event types (§34.2), extensible per §28 without renaming
/// existing variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    WorkspaceAdded,
    WorkspaceRemoved,
    FileAdded,
    FileUpdated,
    FileDeleted,
    IndexCompleted,
    JobFailed,
    ModelLoaded,
    ModelUnavailable,
    ChatStarted,
    ConceptUpdated,
    MemoryUpdated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEvent {
    pub id: Option<EventId>,
    pub event_type: EventType,
    pub payload: serde_json::Value,
    pub occurred_at: String,
}
