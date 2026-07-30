//! `JobRepository` interface (§33.14, §36, §21): backing store for the
//! Background Job System. Enables an app restart mid-index to resume rather
//! than restart from zero (§21). This crate owns the interface only --
//! ownership of the `jobs` table per §33.14 is "`core-indexing` (job queue
//! implementation shared via interface, described in §36)" -- concrete
//! SQLite storage is implemented by atlas-db and injected at composition
//! time (Dependency Inversion, Governing Principle).
//!
//! This milestone implements the queue mechanism itself (enqueue, dequeue,
//! mark running/succeeded/failed, retry policy) -- per the task scope,
//! actual indexing *work* (OCR/parse/embed) is explicitly out of scope; a
//! job sitting in this queue with `job_type = "index_document"` is simply
//! data until a future milestone's worker consumes it.

use atlas_types::ids::JobId;
use atlas_types::job::{Job, JobStatus};
use atlas_utils::AppError;

pub trait JobRepository: Send + Sync {
    /// Insert a new job in `Queued` status and return it with its assigned
    /// id (§33.14).
    fn enqueue(&self, job: Job) -> Result<Job, AppError>;

    /// Fetch the next queued job in priority order (highest priority
    /// first, then oldest first), without removing it from the table --
    /// callers must call [`mark_running`] to claim it (§21: "resume rather
    /// than restart").
    fn next_queued(&self) -> Result<Option<Job>, AppError>;

    fn find_by_id(&self, id: JobId) -> Result<Option<Job>, AppError>;

    fn list_by_status(&self, status: JobStatus) -> Result<Vec<Job>, AppError>;

    fn mark_running(&self, id: JobId) -> Result<Job, AppError>;

    fn mark_succeeded(&self, id: JobId) -> Result<Job, AppError>;

    /// Record a failure. If `retry_count < max_retries`, the job is
    /// returned to `Queued` (bounded retry policy, §45.1 "Retryable");
    /// otherwise it is left in `Failed`.
    fn mark_failed(&self, id: JobId, error: String) -> Result<Job, AppError>;

    fn cancel(&self, id: JobId) -> Result<Job, AppError>;

    /// All jobs still `Queued` or `Running` at the time this is called --
    /// used at startup (§41 step 7) to resume in-flight work rather than
    /// silently dropping it.
    fn list_resumable(&self) -> Result<Vec<Job>, AppError>;
}
