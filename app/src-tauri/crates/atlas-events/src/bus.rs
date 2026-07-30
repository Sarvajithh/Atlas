//! Event Bus interface (§34.1, §34.3).

use atlas_types::event::{AppEvent, EventType};
use atlas_utils::AppError;

/// A subscriber reacts to published events. Subscribers MUST be idempotent
/// where practical, since replay after a crash may redeliver (§34.3).
pub trait EventSubscriber: Send + Sync {
    fn handle(&self, event: &AppEvent) -> Result<(), AppError>;
}

/// The Event Bus routes and logs events; it never carries business logic
/// itself (§34.1). Concrete implementations persist to the `events` table
/// (§33.15) via a dependency-inverted adapter supplied at composition time.
pub trait EventBus: Send + Sync {
    /// Publish an event to all registered subscribers and persist it.
    fn publish(&self, event: AppEvent) -> Result<(), AppError>;

    /// Register a subscriber for future published events, of every type
    /// (§34.1: "Deliver events to registered subscribers").
    fn subscribe(&self, subscriber: Box<dyn EventSubscriber>) -> Result<(), AppError>;

    /// Register a subscriber for only the given event types (§34.3:
    /// "A module MAY publish events about its own domain only" -- the
    /// filtered counterpart lets a subscriber likewise only *receive* the
    /// domains it cares about). Default implementation wraps [`subscribe`]
    /// with a type check, so existing implementors of this trait keep
    /// compiling unchanged.
    fn subscribe_filtered(
        &self,
        event_types: Vec<EventType>,
        subscriber: Box<dyn EventSubscriber>,
    ) -> Result<(), AppError> {
        self.subscribe(Box::new(FilteredSubscriber {
            event_types,
            inner: subscriber,
        }))
    }
}

/// Wraps a subscriber so it only receives events whose type is in the
/// configured allow-list (§34.3 filtering).
struct FilteredSubscriber {
    event_types: Vec<EventType>,
    inner: Box<dyn EventSubscriber>,
}

impl EventSubscriber for FilteredSubscriber {
    fn handle(&self, event: &AppEvent) -> Result<(), AppError> {
        if self.event_types.contains(&event.event_type) {
            self.inner.handle(event)
        } else {
            Ok(())
        }
    }
}
