//! Background Job Queue (§21 "Background Workers", §36.1, §33.14). Thin
//! orchestration on top of `JobRepository`: turns a file-system event or
//! any other trigger into a queued `Job` row, so the Indexing Worker Pool
//! (a future milestone) has something durable to consume even across a
//! restart (§21: "resume rather than restart"). This module does not
//! perform any indexing work itself -- enqueueing a job is where this
//! milestone's responsibility ends (task scope: "Queue indexing jobs
//! (without implementing indexing)").

use std::sync::Arc;

use atlas_types::ids::{JobId, WorkspaceId};
use atlas_types::job::{Job, JobStatus};
use atlas_utils::time::now_iso8601;
use atlas_utils::validation::require_non_empty;
use atlas_utils::AppError;

use crate::job_repository::JobRepository;

/// Job type constant for a single-document (re)index request, enqueued by
/// the Folder Watcher (§21) on `FileAdded`/`FileUpdated`, and by the
/// Workspace Engine on initial link (§6.1 "Indexing (initial)"). Kept as a
/// `pub const` string (not a hardcoded literal scattered across call
/// sites) so every producer/consumer agrees on the same value (Governing
/// Principle: no hardcoded configuration duplicated ad hoc).
pub const JOB_TYPE_INDEX_DOCUMENT: &str = "index_document";

/// Job type for the Concept Graph extraction step (Phase 5, §20). Enqueued
/// by the Indexing Worker itself, immediately after a document finishes
/// indexing with at least one chunk -- never by the Folder Watcher
/// directly, since extraction needs chunks that only exist once indexing
/// has actually run. Kept as its own job (rather than folded into
/// `index_document`) so extraction genuinely runs as an async background
/// step after embedding, not synchronously inside the parse/OCR/chunk/
/// embed pipeline: a slow or failing extraction call never blocks or fails
/// the document's own indexed/parsed status.
pub const JOB_TYPE_EXTRACT_CONCEPTS: &str = "extract_concepts";

/// Default job priority (mid-range on an arbitrary but documented scale)
/// and default retry budget. These are the sane defaults §37.2 talks about
/// for model assignment, applied here to job scheduling -- callers may
/// override either value; nothing about them is baked into `JobQueue`'s
/// logic itself.
pub const DEFAULT_PRIORITY: i32 = 0;
pub const DEFAULT_MAX_RETRIES: u32 = 3;

pub struct JobQueue {
    repository: Arc<dyn JobRepository>,
}

impl JobQueue {
    pub fn new(repository: Arc<dyn JobRepository>) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &Arc<dyn JobRepository> {
        &self.repository
    }

    /// Enqueue an indexing job for a single file within a workspace. The
    /// payload is intentionally minimal (workspace + relative path) --
    /// what a future indexing worker does with it is out of this
    /// milestone's scope.
    pub fn enqueue_index_job(
        &self,
        workspace_id: WorkspaceId,
        relative_path: &str,
    ) -> Result<Job, AppError> {
        require_non_empty("relative_path", relative_path)?;

        let job = Job {
            id: JobId(0),
            job_type: JOB_TYPE_INDEX_DOCUMENT.to_string(),
            payload: serde_json::json!({
                "workspace_id": workspace_id.0,
                "relative_path": relative_path,
            }),
            status: JobStatus::Queued,
            priority: DEFAULT_PRIORITY,
            retry_count: 0,
            max_retries: DEFAULT_MAX_RETRIES,
            created_at: now_iso8601(),
            started_at: None,
            completed_at: None,
            error: None,
        };
        self.repository.enqueue(job)
    }

    /// Enqueue a Concept Graph extraction job for one already-indexed
    /// document (see [`JOB_TYPE_EXTRACT_CONCEPTS`]).
    pub fn enqueue_extract_concepts_job(
        &self,
        workspace_id: WorkspaceId,
        document_id: atlas_types::ids::DocumentId,
    ) -> Result<Job, AppError> {
        let job = Job {
            id: JobId(0),
            job_type: JOB_TYPE_EXTRACT_CONCEPTS.to_string(),
            payload: serde_json::json!({
                "workspace_id": workspace_id.0,
                "document_id": document_id.0,
            }),
            status: JobStatus::Queued,
            priority: DEFAULT_PRIORITY,
            retry_count: 0,
            max_retries: DEFAULT_MAX_RETRIES,
            created_at: now_iso8601(),
            started_at: None,
            completed_at: None,
            error: None,
        };
        self.repository.enqueue(job)
    }

    pub fn next_queued(&self) -> Result<Option<Job>, AppError> {
        self.repository.next_queued()
    }

    pub fn mark_running(&self, id: JobId) -> Result<Job, AppError> {
        self.repository.mark_running(id)
    }

    pub fn mark_succeeded(&self, id: JobId) -> Result<Job, AppError> {
        self.repository.mark_succeeded(id)
    }

    pub fn mark_failed(&self, id: JobId, error: String) -> Result<Job, AppError> {
        self.repository.mark_failed(id, error)
    }

    pub fn cancel(&self, id: JobId) -> Result<Job, AppError> {
        self.repository.cancel(id)
    }

    /// §41 step 7: "resume any `jobs` rows left in-flight from a prior
    /// session".
    pub fn resumable_jobs(&self) -> Result<Vec<Job>, AppError> {
        self.repository.list_resumable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::InMemoryJobRepository;

    fn queue() -> JobQueue {
        JobQueue::new(Arc::new(InMemoryJobRepository::new()))
    }

    #[test]
    fn enqueue_index_job_creates_a_queued_job_with_expected_payload() {
        let queue = queue();
        let job = queue
            .enqueue_index_job(WorkspaceId(1), "notes/chapter1.pdf")
            .unwrap();

        assert_eq!(job.job_type, JOB_TYPE_INDEX_DOCUMENT);
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.payload["relative_path"], "notes/chapter1.pdf");
        assert_eq!(job.payload["workspace_id"], 1);
    }

    #[test]
    fn enqueue_extract_concepts_job_creates_a_queued_job_with_expected_payload() {
        let queue = queue();
        let job = queue
            .enqueue_extract_concepts_job(WorkspaceId(1), atlas_types::ids::DocumentId(7))
            .unwrap();

        assert_eq!(job.job_type, JOB_TYPE_EXTRACT_CONCEPTS);
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.payload["workspace_id"], 1);
        assert_eq!(job.payload["document_id"], 7);
    }

    #[test]
    fn enqueue_index_job_rejects_empty_relative_path() {
        let queue = queue();
        assert!(queue.enqueue_index_job(WorkspaceId(1), "").is_err());
    }

    #[test]
    fn next_queued_returns_jobs_in_fifo_order_for_equal_priority() {
        let queue = queue();
        queue.enqueue_index_job(WorkspaceId(1), "a.pdf").unwrap();
        queue.enqueue_index_job(WorkspaceId(1), "b.pdf").unwrap();

        let first = queue.next_queued().unwrap().unwrap();
        assert_eq!(first.payload["relative_path"], "a.pdf");
    }

    #[test]
    fn mark_running_then_succeeded_updates_status() {
        let queue = queue();
        let job = queue.enqueue_index_job(WorkspaceId(1), "a.pdf").unwrap();
        let running = queue.mark_running(job.id).unwrap();
        assert_eq!(running.status, JobStatus::Running);

        let done = queue.mark_succeeded(job.id).unwrap();
        assert_eq!(done.status, JobStatus::Succeeded);
    }

    #[test]
    fn mark_failed_requeues_while_retries_remain_then_finally_fails() {
        let queue = queue();
        let job = queue.enqueue_index_job(WorkspaceId(1), "a.pdf").unwrap();

        let mut current = job.clone();
        for _ in 0..job.max_retries {
            queue.mark_running(current.id).unwrap();
            current = queue.mark_failed(current.id, "boom".to_string()).unwrap();
            assert_eq!(current.status, JobStatus::Queued);
        }

        queue.mark_running(current.id).unwrap();
        let final_state = queue.mark_failed(current.id, "boom again".to_string()).unwrap();
        assert_eq!(final_state.status, JobStatus::Failed);
        assert_eq!(final_state.retry_count, job.max_retries + 1);
    }

    #[test]
    fn resumable_jobs_includes_queued_and_running_but_not_terminal() {
        let queue = queue();
        let queued = queue.enqueue_index_job(WorkspaceId(1), "a.pdf").unwrap();
        let running = queue.enqueue_index_job(WorkspaceId(1), "b.pdf").unwrap();
        queue.mark_running(running.id).unwrap();
        let succeeded = queue.enqueue_index_job(WorkspaceId(1), "c.pdf").unwrap();
        queue.mark_running(succeeded.id).unwrap();
        queue.mark_succeeded(succeeded.id).unwrap();

        let resumable = queue.resumable_jobs().unwrap();
        let ids: Vec<_> = resumable.iter().map(|j| j.id).collect();
        assert!(ids.contains(&queued.id));
        assert!(ids.contains(&running.id));
        assert!(!ids.contains(&succeeded.id));
    }

    #[test]
    fn cancel_marks_job_cancelled() {
        let queue = queue();
        let job = queue.enqueue_index_job(WorkspaceId(1), "a.pdf").unwrap();
        let cancelled = queue.cancel(job.id).unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
    }
}
