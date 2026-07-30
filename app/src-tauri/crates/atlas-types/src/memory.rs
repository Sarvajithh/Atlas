//! Student Memory shapes (§7.3, §19, §33.7-33.11, §33.16-§33.18).

use serde::{Deserialize, Serialize};

use crate::ids::{
    AnnotationId, BookmarkId, ConceptNodeId, DocumentId, RevisionHistoryId, WorkspaceId,
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
