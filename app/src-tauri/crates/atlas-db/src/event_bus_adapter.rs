//! SQLite-backed persistence for the Event Bus (§34, §33.15). The Event Bus
//! interface itself is owned by atlas-events; this adapter provides the
//! durable append-log implementation, wired in by atlas-core.

use std::sync::Mutex;

use atlas_events::{EventBus, EventSubscriber};
use atlas_types::event::{AppEvent, EventType};
use atlas_types::ids::EventId;
use atlas_utils::AppError;
use rusqlite::params;

use crate::connection::SqliteConnection;

pub struct SqliteEventBus {
    connection: SqliteConnection,
    subscribers: Mutex<Vec<Box<dyn EventSubscriber>>>,
}

impl SqliteEventBus {
    pub fn new(connection: SqliteConnection) -> Self {
        Self {
            connection,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers
            .lock()
            .map(|s| s.len())
            .unwrap_or_default()
    }
}

fn event_type_to_str(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::WorkspaceAdded => "WorkspaceAdded",
        EventType::WorkspaceRemoved => "WorkspaceRemoved",
        EventType::FileAdded => "FileAdded",
        EventType::FileUpdated => "FileUpdated",
        EventType::FileDeleted => "FileDeleted",
        EventType::IndexCompleted => "IndexCompleted",
        EventType::JobFailed => "JobFailed",
        EventType::ModelLoaded => "ModelLoaded",
        EventType::ModelUnavailable => "ModelUnavailable",
        EventType::ChatStarted => "ChatStarted",
        EventType::ConceptUpdated => "ConceptUpdated",
        EventType::MemoryUpdated => "MemoryUpdated",
    }
}

impl EventBus for SqliteEventBus {
    fn publish(&self, event: AppEvent) -> Result<(), AppError> {
        // §34.1: "Persist events to the `events` table... Never carry
        // business logic itself -- the bus routes and logs; subscribers
        // act." Persist first so a subscriber failure never loses the
        // durable record of what happened.
        let persisted = {
            let conn = self.connection.lock()?;
            conn.execute(
                "INSERT INTO events (event_type, payload, occurred_at) VALUES (?1, ?2, ?3)",
                params![
                    event_type_to_str(&event.event_type),
                    event.payload.to_string(),
                    event.occurred_at,
                ],
            )
            .map_err(|e| AppError::storage(format!("event publish failed: {e}")))?;
            conn.last_insert_rowid()
        };

        let event_with_id = AppEvent {
            id: Some(EventId(persisted)),
            ..event
        };

        // §34.3: subscribers must not assume delivery order relative to
        // each other; we deliver in registration order, which is *a* valid
        // order but not a contract subscribers may rely on.
        let subscribers = self
            .subscribers
            .lock()
            .map_err(|_| AppError::storage("event subscriber lock poisoned"))?;
        for subscriber in subscribers.iter() {
            subscriber.handle(&event_with_id)?;
        }
        Ok(())
    }

    fn subscribe(&self, subscriber: Box<dyn EventSubscriber>) -> Result<(), AppError> {
        self.subscribers
            .lock()
            .map_err(|_| AppError::storage("event subscriber lock poisoned"))?
            .push(subscriber);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingSubscriber {
        count: Arc<AtomicUsize>,
    }

    impl EventSubscriber for CountingSubscriber {
        fn handle(&self, _event: &AppEvent) -> Result<(), AppError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn bus() -> SqliteEventBus {
        SqliteEventBus::new(SqliteConnection::open(":memory:"))
    }

    fn sample_event(event_type: EventType) -> AppEvent {
        AppEvent {
            id: None,
            event_type,
            payload: serde_json::json!({"k": "v"}),
            occurred_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn publish_persists_the_event_row() {
        let bus = bus();
        bus.publish(sample_event(EventType::WorkspaceAdded)).unwrap();

        let conn = bus.connection().lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn subscriber_receives_every_published_event() {
        let bus = bus();
        let count = Arc::new(AtomicUsize::new(0));
        bus.subscribe(Box::new(CountingSubscriber {
            count: count.clone(),
        }))
        .unwrap();

        bus.publish(sample_event(EventType::WorkspaceAdded)).unwrap();
        bus.publish(sample_event(EventType::FileAdded)).unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn published_event_delivered_to_subscriber_carries_an_assigned_id() {
        let bus = bus();
        let seen_id = Arc::new(std::sync::Mutex::new(None));
        let seen_id_clone = seen_id.clone();

        struct IdCapturingSubscriber {
            seen_id: Arc<std::sync::Mutex<Option<EventId>>>,
        }
        impl EventSubscriber for IdCapturingSubscriber {
            fn handle(&self, event: &AppEvent) -> Result<(), AppError> {
                *self.seen_id.lock().unwrap() = event.id;
                Ok(())
            }
        }

        bus.subscribe(Box::new(IdCapturingSubscriber {
            seen_id: seen_id_clone,
        }))
        .unwrap();
        bus.publish(sample_event(EventType::FileDeleted)).unwrap();

        assert!(seen_id.lock().unwrap().is_some());
    }
}
