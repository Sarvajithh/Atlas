//! Testing infrastructure for this crate (§30): dependency-free, in-memory
//! implementations of every repository trait defined here, for unit tests
//! that don't need `atlas-db`/SQLite.

use std::sync::Mutex;

use atlas_types::chat::{ChatMessage, ChatSession};
use atlas_types::ids::{
    AnnotationId, BookmarkId, ChatSessionId, ConceptNodeId, DocumentId, WorkspaceId,
};
use atlas_types::memory::{
    AnalyticsPoint, Annotation, Bookmark, LearningProgress, RevisionHistoryEntry,
};
use atlas_utils::AppError;

use crate::{
    AnalyticsRepository, AnnotationRepository, BookmarkRepository, ChatRepository,
    LearningProgressRepository,
};

fn lock_err(what: &str) -> AppError {
    AppError::user(format!("{what} lock poisoned"))
}

#[derive(Default)]
pub struct InMemoryAnnotationRepository {
    annotations: Mutex<Vec<Annotation>>,
}

impl InMemoryAnnotationRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AnnotationRepository for InMemoryAnnotationRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Annotation>, AppError> {
        let items = self
            .annotations
            .lock()
            .map_err(|_| lock_err("annotation"))?;
        Ok(items
            .iter()
            .filter(|a| a.document_id == document_id)
            .cloned()
            .collect())
    }

    fn insert(&self, annotation: Annotation) -> Result<Annotation, AppError> {
        let mut items = self
            .annotations
            .lock()
            .map_err(|_| lock_err("annotation"))?;
        items.push(annotation.clone());
        Ok(annotation)
    }

    fn update(&self, annotation: Annotation) -> Result<Annotation, AppError> {
        let mut items = self
            .annotations
            .lock()
            .map_err(|_| lock_err("annotation"))?;
        if let Some(existing) = items.iter_mut().find(|a| a.id == annotation.id) {
            *existing = annotation.clone();
        }
        Ok(annotation)
    }

    fn delete(&self, id: AnnotationId) -> Result<(), AppError> {
        let mut items = self
            .annotations
            .lock()
            .map_err(|_| lock_err("annotation"))?;
        items.retain(|a| a.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryBookmarkRepository {
    bookmarks: Mutex<Vec<Bookmark>>,
}

impl InMemoryBookmarkRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BookmarkRepository for InMemoryBookmarkRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Bookmark>, AppError> {
        let items = self.bookmarks.lock().map_err(|_| lock_err("bookmark"))?;
        Ok(items
            .iter()
            .filter(|b| b.document_id == document_id)
            .cloned()
            .collect())
    }

    fn insert(&self, bookmark: Bookmark) -> Result<Bookmark, AppError> {
        let mut items = self.bookmarks.lock().map_err(|_| lock_err("bookmark"))?;
        items.push(bookmark.clone());
        Ok(bookmark)
    }

    fn delete(&self, id: BookmarkId) -> Result<(), AppError> {
        let mut items = self.bookmarks.lock().map_err(|_| lock_err("bookmark"))?;
        items.retain(|b| b.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryChatRepository {
    sessions: Mutex<Vec<ChatSession>>,
    messages: Mutex<Vec<ChatMessage>>,
}

impl InMemoryChatRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ChatRepository for InMemoryChatRepository {
    fn list_sessions_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ChatSession>, AppError> {
        let sessions = self.sessions.lock().map_err(|_| lock_err("chat session"))?;
        Ok(sessions
            .iter()
            .filter(|s| s.workspace_id == workspace_id)
            .cloned()
            .collect())
    }

    fn create_session(&self, session: ChatSession) -> Result<ChatSession, AppError> {
        let mut sessions = self.sessions.lock().map_err(|_| lock_err("chat session"))?;
        sessions.push(session.clone());
        Ok(session)
    }

    fn append_message(&self, message: ChatMessage) -> Result<ChatMessage, AppError> {
        let mut messages = self.messages.lock().map_err(|_| lock_err("chat message"))?;
        messages.push(message.clone());
        Ok(message)
    }

    fn list_messages(&self, session_id: ChatSessionId) -> Result<Vec<ChatMessage>, AppError> {
        let messages = self.messages.lock().map_err(|_| lock_err("chat message"))?;
        Ok(messages
            .iter()
            .filter(|m| m.session_id == session_id)
            .cloned()
            .collect())
    }
}

#[derive(Default)]
pub struct InMemoryLearningProgressRepository {
    progress: Mutex<Vec<LearningProgress>>,
    history: Mutex<Vec<RevisionHistoryEntry>>,
}

impl InMemoryLearningProgressRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LearningProgressRepository for InMemoryLearningProgressRepository {
    fn get_progress(
        &self,
        concept_node_id: ConceptNodeId,
    ) -> Result<Option<LearningProgress>, AppError> {
        let progress = self
            .progress
            .lock()
            .map_err(|_| lock_err("learning progress"))?;
        Ok(progress
            .iter()
            .find(|p| p.concept_node_id == concept_node_id)
            .cloned())
    }

    fn upsert_progress(&self, progress: LearningProgress) -> Result<LearningProgress, AppError> {
        let mut items = self
            .progress
            .lock()
            .map_err(|_| lock_err("learning progress"))?;
        if let Some(existing) = items
            .iter_mut()
            .find(|p| p.concept_node_id == progress.concept_node_id)
        {
            *existing = progress.clone();
        } else {
            items.push(progress.clone());
        }
        Ok(progress)
    }

    fn append_revision_history(
        &self,
        entry: RevisionHistoryEntry,
    ) -> Result<RevisionHistoryEntry, AppError> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| lock_err("revision history"))?;
        history.push(entry.clone());
        Ok(entry)
    }

    fn list_revision_history(
        &self,
        concept_node_id: ConceptNodeId,
    ) -> Result<Vec<RevisionHistoryEntry>, AppError> {
        let history = self
            .history
            .lock()
            .map_err(|_| lock_err("revision history"))?;
        Ok(history
            .iter()
            .filter(|h| h.concept_node_id == concept_node_id)
            .cloned()
            .collect())
    }
}

#[derive(Default)]
pub struct InMemoryAnalyticsRepository {
    points: Mutex<Vec<AnalyticsPoint>>,
}

impl InMemoryAnalyticsRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AnalyticsRepository for InMemoryAnalyticsRepository {
    fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AnalyticsPoint>, AppError> {
        let points = self.points.lock().map_err(|_| lock_err("analytics"))?;
        Ok(points
            .iter()
            .filter(|p| p.workspace_id == workspace_id)
            .cloned()
            .collect())
    }

    fn upsert(&self, point: AnalyticsPoint) -> Result<AnalyticsPoint, AppError> {
        let mut points = self.points.lock().map_err(|_| lock_err("analytics"))?;
        points.push(point.clone());
        Ok(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::chat::ChatMode;

    #[test]
    fn annotation_repository_insert_then_list() {
        let repo = InMemoryAnnotationRepository::new();
        repo.insert(Annotation {
            id: AnnotationId(1),
            document_id: DocumentId(1),
            location_ref: "p1".to_string(),
            content: "note".to_string(),
            created_at: "1970-01-01T00:00:00Z".to_string(),
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
        assert_eq!(repo.list_for_document(DocumentId(1)).unwrap().len(), 1);
    }

    #[test]
    fn bookmark_repository_delete_removes_entry() {
        let repo = InMemoryBookmarkRepository::new();
        repo.insert(Bookmark {
            id: BookmarkId(1),
            document_id: DocumentId(1),
            location_ref: "p1".to_string(),
            label: "start".to_string(),
            created_at: "1970-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
        repo.delete(BookmarkId(1)).unwrap();
        assert!(repo.list_for_document(DocumentId(1)).unwrap().is_empty());
    }

    #[test]
    fn chat_repository_sessions_and_messages_round_trip() {
        let repo = InMemoryChatRepository::new();
        repo.create_session(ChatSession {
            id: ChatSessionId(1),
            workspace_id: WorkspaceId(1),
            document_id: None,
            title: "Session".to_string(),
            mode: ChatMode::Normal,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
        assert_eq!(
            repo.list_sessions_for_workspace(WorkspaceId(1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn learning_progress_upsert_replaces_existing_entry() {
        let repo = InMemoryLearningProgressRepository::new();
        repo.upsert_progress(LearningProgress {
            concept_node_id: ConceptNodeId(1),
            mastery_score: 0.5,
            weakness_score: 0.5,
            last_reviewed_at: None,
            attempt_count: 1,
        })
        .unwrap();
        repo.upsert_progress(LearningProgress {
            concept_node_id: ConceptNodeId(1),
            mastery_score: 0.8,
            weakness_score: 0.2,
            last_reviewed_at: None,
            attempt_count: 2,
        })
        .unwrap();

        let progress = repo.get_progress(ConceptNodeId(1)).unwrap().unwrap();
        assert_eq!(progress.attempt_count, 2);
    }

    #[test]
    fn analytics_repository_filters_by_workspace() {
        let repo = InMemoryAnalyticsRepository::new();
        repo.upsert(AnalyticsPoint {
            workspace_id: WorkspaceId(1),
            metric_key: "reviews".to_string(),
            metric_value: 3.0,
            computed_at: "1970-01-01T00:00:00Z".to_string(),
            period: "day".to_string(),
        })
        .unwrap();
        assert_eq!(repo.list_for_workspace(WorkspaceId(1)).unwrap().len(), 1);
        assert!(repo.list_for_workspace(WorkspaceId(2)).unwrap().is_empty());
    }
}
