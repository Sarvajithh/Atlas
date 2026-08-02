//! Background job shapes (§21, §33.14, §36).

use serde::{Deserialize, Serialize};

use crate::ids::JobId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub priority: i32,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

/// The job currently being processed by the Background Indexing Worker,
/// for a single workspace (§21, §36). Surfaced to the UI so a future
/// Learning Progress panel (task scope: "current file") can show it
/// without the frontend re-deriving it from raw `Job` rows itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningIndexingJob {
    pub job_id: JobId,
    pub relative_path: String,
    pub started_at: Option<String>,
    pub retry_count: u32,
}

/// Minimal, read-only snapshot of indexing progress for one workspace,
/// derived entirely from the existing `jobs` table (§33.14) -- no
/// parallel/duplicated state is introduced. Counts are jobs, not
/// documents: a document can be represented by more than one historical
/// job (e.g. a requeue), but the counts below only ever reflect the
/// *current* row per job id, which is what the `jobs` table already
/// stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStatus {
    pub queued: usize,
    pub running: Option<RunningIndexingJob>,
    pub succeeded: usize,
    pub failed: usize,
    /// `queued + (1 if running else 0) + succeeded + failed`.
    pub total: usize,
    /// `(succeeded + failed) / total * 100`, rounded to one decimal place.
    /// `None` when `total == 0` (nothing has ever been queued for this
    /// workspace), so the UI can distinguish "0% done" from "nothing to
    /// do" (§4 acceptance criteria: "progress percentage (if available)").
    pub progress_percent: Option<f32>,
    /// `completed_at` of the most recently succeeded job, if any.
    pub last_indexed_at: Option<String>,
}
