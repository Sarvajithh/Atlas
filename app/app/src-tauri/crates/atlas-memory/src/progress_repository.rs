//! `LearningProgressRepository` interface (§33.17, §33.18). Backs the
//! Planner and weakness-scoring logic. Implemented by atlas-db.

use atlas_types::ids::{ConceptNodeId, DocumentId, FlashcardSetId, QuizId, WorkspaceId};
use atlas_types::memory::{FlashcardSet, LearningProgress, Quiz, RevisionHistoryEntry, RevisionPlan};
use atlas_utils::AppError;

pub trait LearningProgressRepository: Send + Sync {
    fn get_progress(
        &self,
        concept_node_id: ConceptNodeId,
    ) -> Result<Option<LearningProgress>, AppError>;
    fn upsert_progress(&self, progress: LearningProgress) -> Result<LearningProgress, AppError>;
    fn append_revision_history(
        &self,
        entry: RevisionHistoryEntry,
    ) -> Result<RevisionHistoryEntry, AppError>;
    fn list_revision_history(
        &self,
        concept_node_id: ConceptNodeId,
    ) -> Result<Vec<RevisionHistoryEntry>, AppError>;
}

/// Persistence for generated Quiz/Flashcard/RevisionPlan structured
/// records (§ Learning subsystem). Kept as its own trait rather than
/// folded into `LearningProgressRepository` -- these are topic-tagged
/// generated artifacts, distinct from `LearningProgress`'s
/// `ConceptNodeId`-keyed mastery tracking (§33.17/§33.18), which this
/// milestone does not touch. Subject to the same Student Memory
/// non-destructive-deletion guarantee as annotations/bookmarks (§7.3):
/// implementations must never cascade-delete these records on
/// document/workspace removal.
pub trait StudyRepository: Send + Sync {
    fn insert_quiz(&self, quiz: Quiz) -> Result<Quiz, AppError>;
    fn get_quiz(&self, id: QuizId) -> Result<Option<Quiz>, AppError>;
    fn list_quizzes_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<Quiz>, AppError>;
    /// Quizzes generated for a specific document, if `document_id` was
    /// supplied at generation time.
    fn list_quizzes_for_document(&self, document_id: DocumentId) -> Result<Vec<Quiz>, AppError>;

    fn insert_flashcard_set(&self, set: FlashcardSet) -> Result<FlashcardSet, AppError>;
    fn get_flashcard_set(&self, id: FlashcardSetId) -> Result<Option<FlashcardSet>, AppError>;
    fn list_flashcard_sets_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<FlashcardSet>, AppError>;

    fn insert_revision_plan(&self, plan: RevisionPlan) -> Result<RevisionPlan, AppError>;
    fn list_revision_plans_for_workspace(&self, workspace_id: WorkspaceId) -> Result<Vec<RevisionPlan>, AppError>;
}
