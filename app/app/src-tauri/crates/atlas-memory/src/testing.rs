//! Testing infrastructure for this crate (§30): dependency-free, in-memory
//! implementations of every repository trait defined here, for unit tests
//! that don't need `atlas-db`/SQLite.

use std::sync::Mutex;

use atlas_types::chat::{ChatMessage, ChatSession};
use atlas_types::ids::{
    AnnotationId, BookmarkId, ChatSessionId, ConceptNodeId, DocumentId, FlashcardSetId, QuizId,
    RevisionPlanId, WorkspaceId,
};
use atlas_types::memory::{
    AnalyticsPoint, Annotation, Bookmark, FlashcardSet, LearningProgress, Quiz, RevisionHistoryEntry,
    RevisionPlan, WeakTopic,
};
use atlas_utils::AppError;

use crate::{
    AnalyticsRepository, AnnotationRepository, BookmarkRepository, ChatRepository,
    LearningProgressRepository, StudyRepository,
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
    /// `(workspace_id, topic) -> (correct_count, incorrect_count)`.
    weak_topic_stats: Mutex<std::collections::HashMap<(i64, String), (u32, u32)>>,
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

    fn record_quiz_answer(&self, workspace_id: WorkspaceId, topic: &str, correct: bool) -> Result<(), AppError> {
        let mut stats = self.weak_topic_stats.lock().map_err(|_| lock_err("weak topic stats"))?;
        let entry = stats.entry((workspace_id.0, topic.to_string())).or_insert((0, 0));
        if correct {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
        Ok(())
    }

    fn list_weak_topics(&self, workspace_id: WorkspaceId) -> Result<Vec<WeakTopic>, AppError> {
        let stats = self.weak_topic_stats.lock().map_err(|_| lock_err("weak topic stats"))?;
        let mut topics: Vec<WeakTopic> = stats
            .iter()
            .filter(|((wid, _), _)| *wid == workspace_id.0)
            .map(|((_, topic), (correct, incorrect))| {
                let total = correct + incorrect;
                let accuracy = if total == 0 { 0.0 } else { *correct as f32 / total as f32 };
                WeakTopic {
                    topic: topic.clone(),
                    correct_count: *correct,
                    incorrect_count: *incorrect,
                    accuracy,
                }
            })
            .collect();
        topics.sort_by(|a, b| a.accuracy.partial_cmp(&b.accuracy).unwrap_or(std::cmp::Ordering::Equal));
        Ok(topics)
    }
}

#[derive(Default)]
pub struct InMemoryStudyRepository {
    quizzes: Mutex<Vec<Quiz>>,
    flashcard_sets: Mutex<Vec<FlashcardSet>>,
    revision_plans: Mutex<Vec<RevisionPlan>>,
    next_quiz_id: Mutex<i64>,
    next_flashcard_set_id: Mutex<i64>,
    next_revision_plan_id: Mutex<i64>,
}

impl InMemoryStudyRepository {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_id(counter: &Mutex<i64>) -> Result<i64, AppError> {
        let mut n = counter.lock().map_err(|_| lock_err("id counter"))?;
        *n += 1;
        Ok(*n)
    }
}

impl StudyRepository for InMemoryStudyRepository {
    fn insert_quiz(&self, quiz: Quiz) -> Result<Quiz, AppError> {
        let id = QuizId(Self::next_id(&self.next_quiz_id)?);
        let quiz = Quiz { id, ..quiz };
        let mut quizzes = self.quizzes.lock().map_err(|_| lock_err("quiz"))?;
        quizzes.push(quiz.clone());
        Ok(quiz)
    }

    fn get_quiz(&self, id: QuizId) -> Result<Option<Quiz>, AppError> {
        let quizzes = self.quizzes.lock().map_err(|_| lock_err("quiz"))?;
        Ok(quizzes.iter().find(|q| q.id == id).cloned())
    }

    fn list_quizzes_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<Quiz>, AppError> {
        let quizzes = self.quizzes.lock().map_err(|_| lock_err("quiz"))?;
        Ok(quizzes.iter().filter(|q| q.workspace_id == workspace_id).cloned().collect())
    }

    fn list_quizzes_for_document(&self, document_id: DocumentId) -> Result<Vec<Quiz>, AppError> {
        let quizzes = self.quizzes.lock().map_err(|_| lock_err("quiz"))?;
        Ok(quizzes.iter().filter(|q| q.document_id == Some(document_id)).cloned().collect())
    }

    fn insert_flashcard_set(&self, set: FlashcardSet) -> Result<FlashcardSet, AppError> {
        let id = FlashcardSetId(Self::next_id(&self.next_flashcard_set_id)?);
        let set = FlashcardSet { id, ..set };
        let mut sets = self.flashcard_sets.lock().map_err(|_| lock_err("flashcard set"))?;
        sets.push(set.clone());
        Ok(set)
    }

    fn get_flashcard_set(&self, id: FlashcardSetId) -> Result<Option<FlashcardSet>, AppError> {
        let sets = self.flashcard_sets.lock().map_err(|_| lock_err("flashcard set"))?;
        Ok(sets.iter().find(|s| s.id == id).cloned())
    }

    fn list_flashcard_sets_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<FlashcardSet>, AppError> {
        let sets = self.flashcard_sets.lock().map_err(|_| lock_err("flashcard set"))?;
        Ok(sets.iter().filter(|s| s.workspace_id == workspace_id).cloned().collect())
    }

    fn insert_revision_plan(&self, plan: RevisionPlan) -> Result<RevisionPlan, AppError> {
        let id = RevisionPlanId(Self::next_id(&self.next_revision_plan_id)?);
        let plan = RevisionPlan { id, ..plan };
        let mut plans = self.revision_plans.lock().map_err(|_| lock_err("revision plan"))?;
        plans.push(plan.clone());
        Ok(plan)
    }

    fn list_revision_plans_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<RevisionPlan>, AppError> {
        let plans = self.revision_plans.lock().map_err(|_| lock_err("revision plan"))?;
        Ok(plans.iter().filter(|p| p.workspace_id == workspace_id).cloned().collect())
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

    #[test]
    fn weak_topic_aggregate_accumulates_across_recorded_answers() {
        let repo = InMemoryAnalyticsRepository::new();
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Thermodynamics", true).unwrap();
        repo.record_quiz_answer(WorkspaceId(1), "Optics", true).unwrap();

        let weak = repo.list_weak_topics(WorkspaceId(1)).unwrap();
        assert_eq!(weak.len(), 2);
        // Weakest (lowest accuracy) first.
        assert_eq!(weak[0].topic, "Thermodynamics");
        assert_eq!(weak[0].correct_count, 1);
        assert_eq!(weak[0].incorrect_count, 2);
        assert!((weak[0].accuracy - (1.0 / 3.0)).abs() < 1e-6);
        assert_eq!(weak[1].topic, "Optics");
    }

    #[test]
    fn weak_topics_are_scoped_per_workspace() {
        let repo = InMemoryAnalyticsRepository::new();
        repo.record_quiz_answer(WorkspaceId(1), "Topic A", false).unwrap();
        repo.record_quiz_answer(WorkspaceId(2), "Topic A", true).unwrap();
        assert_eq!(repo.list_weak_topics(WorkspaceId(1)).unwrap()[0].correct_count, 0);
        assert_eq!(repo.list_weak_topics(WorkspaceId(2)).unwrap()[0].correct_count, 1);
    }

    #[test]
    fn study_repository_quiz_round_trips_and_assigns_an_id() {
        let repo = InMemoryStudyRepository::new();
        let quiz = repo
            .insert_quiz(Quiz {
                id: QuizId(0),
                workspace_id: WorkspaceId(1),
                document_id: None,
                topic: "Photosynthesis".to_string(),
                questions: vec![],
                created_at: "1970-01-01T00:00:00Z".to_string(),
            })
            .unwrap();
        assert_ne!(quiz.id.0, 0);
        assert_eq!(repo.get_quiz(quiz.id).unwrap().unwrap().topic, "Photosynthesis");
        assert_eq!(repo.list_quizzes_for_workspace(WorkspaceId(1)).unwrap().len(), 1);
        assert!(repo.list_quizzes_for_workspace(WorkspaceId(2)).unwrap().is_empty());
    }

    #[test]
    fn study_repository_flashcard_set_and_revision_plan_round_trip() {
        let repo = InMemoryStudyRepository::new();
        let set = repo
            .insert_flashcard_set(FlashcardSet {
                id: FlashcardSetId(0),
                workspace_id: WorkspaceId(1),
                document_id: None,
                topic: "Cell Biology".to_string(),
                cards: vec![],
                created_at: "1970-01-01T00:00:00Z".to_string(),
            })
            .unwrap();
        assert_ne!(set.id.0, 0);
        assert_eq!(repo.get_flashcard_set(set.id).unwrap().unwrap().topic, "Cell Biology");

        let plan = repo
            .insert_revision_plan(RevisionPlan {
                id: RevisionPlanId(0),
                workspace_id: WorkspaceId(1),
                items: vec![],
                created_at: "1970-01-01T00:00:00Z".to_string(),
            })
            .unwrap();
        assert_ne!(plan.id.0, 0);
        assert_eq!(repo.list_revision_plans_for_workspace(WorkspaceId(1)).unwrap().len(), 1);
    }
}
