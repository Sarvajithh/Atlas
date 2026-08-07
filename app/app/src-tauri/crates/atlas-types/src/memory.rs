//! Student Memory shapes (§7.3, §19, §33.7-33.11, §33.16-§33.18).

use serde::{Deserialize, Serialize};

use crate::ids::{
    AnnotationId, BookmarkId, ConceptNodeId, DocumentId, FlashcardSetId, QuizId,
    RevisionHistoryId, RevisionPlanId, WorkspaceId,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: AnnotationId,
    pub document_id: DocumentId,
    pub location_ref: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: BookmarkId,
    pub document_id: DocumentId,
    pub location_ref: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevisionOutcome {
    Recalled,
    Forgotten,
}

/// Mirrors `revision_history` (§33.17).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionHistoryEntry {
    pub id: RevisionHistoryId,
    pub concept_node_id: ConceptNodeId,
    pub scheduled_at: String,
    pub completed_at: Option<String>,
    pub outcome: Option<RevisionOutcome>,
}

/// Mirrors `learning_progress` (§33.18), the read model for mastery/weakness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningProgress {
    pub concept_node_id: ConceptNodeId,
    pub mastery_score: f32,
    pub weakness_score: f32,
    pub last_reviewed_at: Option<String>,
    pub attempt_count: u32,
}

/// Mirrors `analytics` (§33.16), a materialized/cache table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsPoint {
    pub workspace_id: WorkspaceId,
    pub metric_key: String,
    pub metric_value: f64,
    pub computed_at: String,
    pub period: String,
}

// ---------------------------------------------------------------------
// Quiz / Flashcard / Revision Planner structured output contracts.
//
// Deliberately keyed by a free-text `topic: String` tag rather than
// `ConceptNodeId` -- the Concept Graph crate currently produces zero
// nodes/edges (extraction logic is "deferred to a future milestone" per
// its own source comment), so tying weak-topic detection to it would
// make this feature depend on a subsystem that never emits data. A
// topic tag is supplied by the caller (e.g. the workspace/document
// section the quiz was generated for) and is what
// `analytics_repository`'s weak-topic aggregation groups by. This can
// be migrated to `ConceptNodeId` later if/when the Concept Graph is
// built out, without changing the shape of these structs' consumers.
// ---------------------------------------------------------------------

/// A single model-generated quiz question, per the structured-output
/// contract described in the Learning-subsystem implementation plan.
/// `correct_answer` must be one of `options` verbatim -- validated at
/// parse time in `engines.rs`, not assumed here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub correct_answer: String,
    pub source_citations: Vec<String>,
}

/// A generated quiz, persisted as Student Memory (subject to the same
/// non-destructive-deletion guarantee as annotations/bookmarks) and tagged
/// by workspace/document/topic so it can be retrieved and its results fed
/// into weak-topic aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quiz {
    pub id: QuizId,
    pub workspace_id: WorkspaceId,
    pub document_id: Option<DocumentId>,
    pub topic: String,
    pub questions: Vec<QuizQuestion>,
    pub created_at: String,
}

/// A single model-generated flashcard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flashcard {
    pub front: String,
    pub back: String,
    pub source_citations: Vec<String>,
}

/// A generated set of flashcards, persisted and tagged the same way as
/// [`Quiz`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashcardSet {
    pub id: FlashcardSetId,
    pub workspace_id: WorkspaceId,
    pub document_id: Option<DocumentId>,
    pub topic: String,
    pub cards: Vec<Flashcard>,
    pub created_at: String,
}

/// A real, computed (not model-freeform) weak-topic aggregate: correctness
/// counts for a topic tag, accumulated across every quiz attempt recorded
/// for it. Produced by `analytics_repository`'s aggregation query, not
/// inferred by an LLM each time it's needed (per the implementation plan's
/// explicit requirement).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeakTopic {
    pub topic: String,
    pub correct_count: u32,
    pub incorrect_count: u32,
    /// `correct_count / (correct_count + incorrect_count)`, precomputed so
    /// callers (including the revision-plan prompt) don't each reimplement
    /// the division-by-zero guard for a topic with zero attempts.
    pub accuracy: f32,
}

/// One recommendation within a generated [`RevisionPlan`], targeting a
/// specific weak topic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionPlanItem {
    pub topic: String,
    pub recommendation: String,
    /// Lower number = higher priority (1 is the most urgent), matching the
    /// weakest topics first -- set by the planner prompt's structured
    /// output, not recomputed client-side.
    pub priority: u32,
}

/// A generated revision plan, built from the [`WeakTopic`] aggregate
/// (§ context_builder assembly) rather than operating blind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionPlan {
    pub id: RevisionPlanId,
    pub workspace_id: WorkspaceId,
    pub items: Vec<RevisionPlanItem>,
    pub created_at: String,
}
