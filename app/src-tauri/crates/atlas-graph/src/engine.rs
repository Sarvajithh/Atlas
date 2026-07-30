//! Graph Engine (§14, §20). Construction/extraction logic deferred to a
//! future milestone; this defines the injected dependencies only.

use std::sync::Arc;

use atlas_events::EventBus;

use crate::GraphRepository;

pub struct GraphEngine {
    repository: Arc<dyn GraphRepository>,
    events: Arc<dyn EventBus>,
}

impl GraphEngine {
    pub fn new(repository: Arc<dyn GraphRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repository, events }
    }

    pub fn repository(&self) -> &Arc<dyn GraphRepository> {
        &self.repository
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;
    use atlas_types::ids::WorkspaceId;

    use crate::testing::InMemoryGraphRepository;

    #[test]
    fn engine_exposes_the_injected_dependencies() {
        let repository: Arc<dyn GraphRepository> = Arc::new(InMemoryGraphRepository::new());
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let engine = GraphEngine::new(repository, events);

        assert!(engine
            .repository()
            .list_nodes_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
    }
}
