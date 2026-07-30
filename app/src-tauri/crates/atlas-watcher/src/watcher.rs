//! Folder Watcher skeleton (§21).

use std::sync::Arc;

use atlas_events::EventBus;

/// One instance per active workspace root (§21). Debouncing and OS-level
/// watching implementation are deferred to a future milestone.
pub struct FolderWatcher {
    events: Arc<dyn EventBus>,
}

impl FolderWatcher {
    pub fn new(events: Arc<dyn EventBus>) -> Self {
        Self { events }
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;

    #[test]
    fn watcher_exposes_the_injected_event_bus() {
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let watcher = FolderWatcher::new(events.clone());
        assert!(Arc::ptr_eq(watcher.events(), &events));
    }
}
