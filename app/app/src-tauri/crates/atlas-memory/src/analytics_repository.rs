//! `AnalyticsRepository` interface (§33.16). A materialized/cache table
//! conceptually belonging to AI Cache (§7.2) even though it is derived from
//! Student Memory data. Implemented by atlas-db.

use atlas_types::ids::WorkspaceId;
use atlas_types::memory::{AnalyticsPoint, WeakTopic};
use atlas_utils::AppError;

pub trait AnalyticsRepository: Send + Sync {
    fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AnalyticsPoint>, AppError>;
    fn upsert(&self, point: AnalyticsPoint) -> Result<AnalyticsPoint, AppError>;

    /// Record one quiz-question outcome against a topic tag (§ Learning
    /// subsystem weak-topic detection). Increments a running
    /// correct/incorrect count for `(workspace_id, topic)` -- a real,
    /// incrementally-computed aggregate, not something re-derived by an
    /// LLM on every read. Called once per answered question when a quiz
    /// attempt is submitted.
    fn record_quiz_answer(&self, workspace_id: WorkspaceId, topic: &str, correct: bool) -> Result<(), AppError>;

    /// The computed weak-topic aggregate for a workspace, ordered weakest
    /// (lowest accuracy) first -- this is what the Revision Planner prompt
    /// consumes (`PromptBuilder::build_revision_plan_prompt`) instead of
    /// operating blind.
    fn list_weak_topics(&self, workspace_id: WorkspaceId) -> Result<Vec<WeakTopic>, AppError>;
}
