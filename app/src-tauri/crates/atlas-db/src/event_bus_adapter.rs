//! SQLite-backed persistence for the Event Bus (§34, §33.15). The Event Bus
//! interface itself is owned by atlas-events; this adapter provides the
//! durable append-log implementation, wired in by atlas-core.

use atlas_events::{EventBus, EventSubscriber};
use atlas_types::event::AppEvent;
use atlas_utils::AppError;

use crate::connection::SqliteConnection;

pub struct SqliteEventBus {
    connection: SqliteConnection,
    subscribers: Vec<Box<dyn EventSubscriber>>,
}

impl SqliteEventBus {
    pub fn new(connection: SqliteConnection) -> Self {
        Self {
            connection,
            subscribers: Vec::new(),
        }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl EventBus for SqliteEventBus {
    fn publish(&self, _event: AppEvent) -> Result<(), AppError> {
        unimplemented!("event persistence + dispatch is out of scope for this milestone")
    }

    fn subscribe(&self, _subscriber: Box<dyn EventSubscriber>) -> Result<(), AppError> {
        unimplemented!("subscriber registration is out of scope for this milestone")
    }
}
