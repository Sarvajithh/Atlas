//! `LearningProgressRepository` interface (§33.17, §33.18). Backs the
//! Planner and weakness-scoring logic. Implemented by atlas-db.

use atlas_types::ids::ConceptNodeId;
use atlas_types::memory::{LearningProgress, RevisionHistoryEntry};
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
