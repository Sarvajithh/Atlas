//! Workspace lifecycle transitions (§6.1):
//! Unlinked -> Linking -> Indexing (initial) -> Active -> Archived -> (Unlinked)
//!
//! This module defines the `WorkspaceEngine` shape and its dependency on
//! `WorkspaceRepository` only. Transition logic is intentionally left
//! unimplemented for this milestone (skeleton only).

use std::sync::Arc;

use atlas_events::EventBus;

use crate::repository::WorkspaceRepository;

/// High-level module depending only on interfaces (Governing Principle,
/// Dependency Inversion). Concrete repository/event-bus implementations are
/// injected by atlas-core at composition time.
pub struct WorkspaceEngine {
    repository: Arc<dyn WorkspaceRepository>,
    events: Arc<dyn EventBus>,
}

impl WorkspaceEngine {
    pub fn new(repository: Arc<dyn WorkspaceRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repository, events }
    }

    /// Access to the injected repository, for future lifecycle methods.
    pub fn repository(&self) -> &Arc<dyn WorkspaceRepository> {
        &self.repository
    }

    /// Access to the injected event bus, for future lifecycle methods.
    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;
    use atlas_utils::AppError;

    use crate::testing::InMemoryWorkspaceRepository;

    #[test]
    fn engine_exposes_the_injected_dependencies() {
        let repository: Arc<dyn WorkspaceRepository> = Arc::new(InMemoryWorkspaceRepository::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let engine = WorkspaceEngine::new(repository.clone(), events.clone());

        // Accessors return the same dependencies that were injected
        // (Dependency Inversion, Governing Principle) -- this is what the
        // skeleton milestone guarantees; lifecycle transition logic itself
        // is out of scope for this task.
        assert!(engine.repository().list().unwrap().is_empty());
        assert!(engine.events().subscribe(Box::new(NoopSubscriber)).is_ok());
    }

    struct NoopSubscriber;
    impl atlas_events::EventSubscriber for NoopSubscriber {
        fn handle(&self, _event: &atlas_types::event::AppEvent) -> Result<(), AppError> {
            Ok(())
        }
    }
}
