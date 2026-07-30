//! An in-process `EventBus` implementation (§34.1: "no external message
//! broker -- this stays local-first"). Dispatch is synchronous: a
//! subscriber's `handle` runs on the publisher's call stack before
//! `publish` returns. This satisfies §34.1 ("in-process") without adding an
//! async runtime dependency that is not otherwise part of the frozen stack
//! (§5, §28.5); a future amendment could introduce async dispatch if a
//! concrete need for it arises.
//!
//! This is also the crate's own testing double: consumers that need an
//! `EventBus` in a unit test (rather than the SQLite-backed adapter in
//! `atlas-db`) can use [`InMemoryEventBus`] directly (§30: "Testing
//! Infrastructure").

use std::sync::Mutex;

use atlas_types::event::AppEvent;
use atlas_utils::AppError;

use crate::bus::{EventBus, EventSubscriber};

/// A minimal, dependency-free in-process event bus. Keeps a durable-for-
/// the-process-lifetime log of published events (mirroring the intent of
/// the `events` table, §33.15, without touching SQLite -- persistence is
/// `atlas-db`'s responsibility) so tests can assert on what was published.
pub struct InMemoryEventBus {
    subscribers: Mutex<Vec<Box<dyn EventSubscriber>>>,
    log: Mutex<Vec<AppEvent>>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Mutex::new(Vec::new()),
            log: Mutex::new(Vec::new()),
        }
    }

    /// The events published so far, in publish order. Intended for test
    /// assertions (§30).
    pub fn published_events(&self) -> Vec<AppEvent> {
        self.log.lock().expect("event log lock poisoned").clone()
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers
            .lock()
            .expect("subscriber lock poisoned")
            .len()
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus for InMemoryEventBus {
    fn publish(&self, event: AppEvent) -> Result<(), AppError> {
        self.log
            .lock()
            .map_err(|_| AppError::user("event log lock poisoned"))?
            .push(event.clone());

        let subscribers = self
            .subscribers
            .lock()
            .map_err(|_| AppError::user("subscriber lock poisoned"))?;
        for subscriber in subscribers.iter() {
            subscriber.handle(&event)?;
        }
        Ok(())
    }

    fn subscribe(&self, subscriber: Box<dyn EventSubscriber>) -> Result<(), AppError> {
        self.subscribers
            .lock()
            .map_err(|_| AppError::user("subscriber lock poisoned"))?
            .push(subscriber);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use atlas_types::event::EventType;

    use super::*;

    struct CountingSubscriber {
        count: Arc<AtomicUsize>,
    }

    impl EventSubscriber for CountingSubscriber {
        fn handle(&self, _event: &AppEvent) -> Result<(), AppError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn sample_event(event_type: EventType) -> AppEvent {
        AppEvent {
            id: None,
            event_type,
            payload: serde_json::json!({}),
            occurred_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn publish_with_no_subscribers_still_logs_event() {
        let bus = InMemoryEventBus::new();
        bus.publish(sample_event(EventType::WorkspaceAdded))
            .unwrap();
        assert_eq!(bus.published_events().len(), 1);
    }

    #[test]
    fn subscriber_receives_every_published_event() {
        let bus = InMemoryEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        bus.subscribe(Box::new(CountingSubscriber {
            count: count.clone(),
        }))
        .unwrap();

        bus.publish(sample_event(EventType::WorkspaceAdded))
            .unwrap();
        bus.publish(sample_event(EventType::FileAdded)).unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn filtered_subscriber_only_receives_matching_event_types() {
        let bus = InMemoryEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        bus.subscribe_filtered(
            vec![EventType::FileAdded],
            Box::new(CountingSubscriber {
                count: count.clone(),
            }),
        )
        .unwrap();

        bus.publish(sample_event(EventType::WorkspaceAdded))
            .unwrap();
        bus.publish(sample_event(EventType::FileAdded)).unwrap();
        bus.publish(sample_event(EventType::FileDeleted)).unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn subscriber_count_reflects_registrations() {
        let bus = InMemoryEventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
        bus.subscribe(Box::new(CountingSubscriber {
            count: Arc::new(AtomicUsize::new(0)),
        }))
        .unwrap();
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn published_events_preserve_publish_order() {
        let bus = InMemoryEventBus::new();
        bus.publish(sample_event(EventType::WorkspaceAdded))
            .unwrap();
        bus.publish(sample_event(EventType::WorkspaceRemoved))
            .unwrap();

        let events = bus.published_events();
        assert_eq!(events[0].event_type, EventType::WorkspaceAdded);
        assert_eq!(events[1].event_type, EventType::WorkspaceRemoved);
    }
}
