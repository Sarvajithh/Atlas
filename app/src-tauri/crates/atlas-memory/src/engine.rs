//! Memory Engine (§14.1): reads/writes Student Memory, scores weaknesses.
//! Depends only on the repository interfaces defined in this crate and the
//! Event Bus; scoring logic itself is deferred to a future milestone.

use std::sync::Arc;

use atlas_events::EventBus;

use crate::{
    AnalyticsRepository, AnnotationRepository, BookmarkRepository, ChatRepository,
    LearningProgressRepository,
};

pub struct MemoryEngine {
    annotations: Arc<dyn AnnotationRepository>,
    bookmarks: Arc<dyn BookmarkRepository>,
    chat: Arc<dyn ChatRepository>,
    progress: Arc<dyn LearningProgressRepository>,
    analytics: Arc<dyn AnalyticsRepository>,
    events: Arc<dyn EventBus>,
}

impl MemoryEngine {
    pub fn new(
        annotations: Arc<dyn AnnotationRepository>,
        bookmarks: Arc<dyn BookmarkRepository>,
        chat: Arc<dyn ChatRepository>,
        progress: Arc<dyn LearningProgressRepository>,
        analytics: Arc<dyn AnalyticsRepository>,
        events: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            annotations,
            bookmarks,
            chat,
            progress,
            analytics,
            events,
        }
    }

    pub fn annotations(&self) -> &Arc<dyn AnnotationRepository> {
        &self.annotations
    }

    pub fn bookmarks(&self) -> &Arc<dyn BookmarkRepository> {
        &self.bookmarks
    }

    pub fn chat(&self) -> &Arc<dyn ChatRepository> {
        &self.chat
    }

    pub fn progress(&self) -> &Arc<dyn LearningProgressRepository> {
        &self.progress
    }

    pub fn analytics(&self) -> &Arc<dyn AnalyticsRepository> {
        &self.analytics
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;
    use atlas_types::ids::{DocumentId, WorkspaceId};

    use crate::testing::{
        InMemoryAnalyticsRepository, InMemoryAnnotationRepository, InMemoryBookmarkRepository,
        InMemoryChatRepository, InMemoryLearningProgressRepository,
    };

    #[test]
    fn engine_exposes_all_injected_dependencies() {
        let engine = MemoryEngine::new(
            Arc::new(InMemoryAnnotationRepository::new()),
            Arc::new(InMemoryBookmarkRepository::new()),
            Arc::new(InMemoryChatRepository::new()),
            Arc::new(InMemoryLearningProgressRepository::new()),
            Arc::new(InMemoryAnalyticsRepository::new()),
            Arc::new(InMemoryEventBus::new()),
        );

        assert!(engine
            .annotations()
            .list_for_document(DocumentId(1))
            .unwrap()
            .is_empty());
        assert!(engine
            .bookmarks()
            .list_for_document(DocumentId(1))
            .unwrap()
            .is_empty());
        assert!(engine
            .chat()
            .list_sessions_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
        assert!(engine
            .analytics()
            .list_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
    }
}
