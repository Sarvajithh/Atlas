//! SQLite-backed `JobRepository` (§33.14, §36, §21). Implements the
//! interface owned by atlas-indexer; the domain crate never depends on
//! this crate directly.

use atlas_indexer::JobRepository;
use atlas_types::ids::JobId;
use atlas_types::job::{Job, JobStatus};
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

use crate::connection::SqliteConnection;

pub struct SqliteJobRepository {
    connection: SqliteConnection,
}

impl SqliteJobRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

fn status_to_str(status: &JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

fn status_from_str(value: &str) -> Result<JobStatus, AppError> {
    match value {
        "queued" => Ok(JobStatus::Queued),
        "running" => Ok(JobStatus::Running),
        "succeeded" => Ok(JobStatus::Succeeded),
        "failed" => Ok(JobStatus::Failed),
        "cancelled" => Ok(JobStatus::Cancelled),
        other => Err(AppError::storage(format!(
            "unrecognized job status in database: {other}"
        ))),
    }
}

#[allow(clippy::type_complexity)]
type JobRow = (
    i64,
    String,
    String,
    String,
    i32,
    u32,
    u32,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<JobRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn tuple_to_job(tuple: JobRow) -> Result<Job, AppError> {
    let (
        id,
        job_type,
        payload,
        status,
        priority,
        retry_count,
        max_retries,
        created_at,
        started_at,
        completed_at,
        error,
    ) = tuple;
    Ok(Job {
        id: JobId(id),
        job_type,
        payload: serde_json::from_str(&payload)
            .map_err(|e| AppError::storage(format!("invalid job payload JSON: {e}")))?,
        status: status_from_str(&status)?,
        priority,
        retry_count,
        max_retries,
        created_at,
        started_at,
        completed_at,
        error,
    })
}

const SELECT_COLUMNS: &str = "id, job_type, payload, status, priority, retry_count, max_retries, created_at, started_at, completed_at, error FROM jobs";

impl SqliteJobRepository {
    fn update_status(
        &self,
        id: JobId,
        mutate: impl FnOnce(&mut Job),
    ) -> Result<Job, AppError> {
        let mut job = self
            .find_by_id(id)?
            .ok_or_else(|| AppError::user(format!("job {id:?} not found")))?;
        mutate(&mut job);

        let conn = self.connection.lock()?;
        conn.execute(
            "UPDATE jobs SET status = ?1, retry_count = ?2, started_at = ?3, completed_at = ?4, error = ?5
             WHERE id = ?6",
            params![
                status_to_str(&job.status),
                job.retry_count,
                job.started_at,
                job.completed_at,
                job.error,
                id.0,
            ],
        )
        .map_err(|e| AppError::storage(format!("job status update failed: {e}")))?;
        Ok(job)
    }
}

impl JobRepository for SqliteJobRepository {
    fn enqueue(&self, job: Job) -> Result<Job, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO jobs (job_type, payload, status, priority, retry_count, max_retries, created_at, started_at, completed_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                job.job_type,
                job.payload.to_string(),
                status_to_str(&job.status),
                job.priority,
                job.retry_count,
                job.max_retries,
                job.created_at,
                job.started_at,
                job.completed_at,
                job.error,
            ],
        )
        .map_err(|e| AppError::storage(format!("job enqueue failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(Job {
            id: JobId(id),
            ..job
        })
    }

    fn next_queued(&self) -> Result<Option<Job>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                &format!(
                    "SELECT {SELECT_COLUMNS} WHERE status = 'queued' ORDER BY priority DESC, id ASC LIMIT 1"
                ),
                [],
                row_to_job,
            )
            .optional()
            .map_err(|e| AppError::storage(format!("job next_queued failed: {e}")))?;
        result.map(tuple_to_job).transpose()
    }

    fn find_by_id(&self, id: JobId) -> Result<Option<Job>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} WHERE id = ?1"),
                params![id.0],
                row_to_job,
            )
            .optional()
            .map_err(|e| AppError::storage(format!("job find_by_id failed: {e}")))?;
        result.map(tuple_to_job).transpose()
    }

    fn list_by_status(&self, status: JobStatus) -> Result<Vec<Job>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!("SELECT {SELECT_COLUMNS} WHERE status = ?1 ORDER BY id ASC"))
            .map_err(|e| AppError::storage(format!("job list_by_status prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![status_to_str(&status)], row_to_job)
            .map_err(|e| AppError::storage(format!("job list_by_status query failed: {e}")))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(tuple_to_job(
                row.map_err(|e| AppError::storage(format!("job row read failed: {e}")))?,
            )?);
        }
        Ok(jobs)
    }

    fn mark_running(&self, id: JobId) -> Result<Job, AppError> {
        self.update_status(id, |job| {
            job.status = JobStatus::Running;
            job.started_at = Some(now_iso8601());
        })
    }

    fn mark_succeeded(&self, id: JobId) -> Result<Job, AppError> {
        self.update_status(id, |job| {
            job.status = JobStatus::Succeeded;
            job.completed_at = Some(now_iso8601());
            job.error = None;
        })
    }

    fn mark_failed(&self, id: JobId, error: String) -> Result<Job, AppError> {
        self.update_status(id, |job| {
            job.retry_count += 1;
            job.error = Some(error);
            if job.retry_count <= job.max_retries {
                job.status = JobStatus::Queued;
                job.started_at = None;
            } else {
                job.status = JobStatus::Failed;
                job.completed_at = Some(now_iso8601());
            }
        })
    }

    fn cancel(&self, id: JobId) -> Result<Job, AppError> {
        self.update_status(id, |job| {
            job.status = JobStatus::Cancelled;
            job.completed_at = Some(now_iso8601());
        })
    }

    fn list_resumable(&self) -> Result<Vec<Job>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} WHERE status IN ('queued', 'running') ORDER BY id ASC"
            ))
            .map_err(|e| AppError::storage(format!("job list_resumable prepare failed: {e}")))?;
        let rows = stmt
            .query_map([], row_to_job)
            .map_err(|e| AppError::storage(format!("job list_resumable query failed: {e}")))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(tuple_to_job(
                row.map_err(|e| AppError::storage(format!("job row read failed: {e}")))?,
            )?);
        }
        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> SqliteJobRepository {
        SqliteJobRepository::new(SqliteConnection::open(":memory:"))
    }

    fn sample() -> Job {
        Job {
            id: JobId(0),
            job_type: "index_document".to_string(),
            payload: serde_json::json!({"relative_path": "a.pdf"}),
            status: JobStatus::Queued,
            priority: 0,
            retry_count: 0,
            max_retries: 3,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    #[test]
    fn enqueue_assigns_id_and_persists() {
        let repo = repo();
        let job = repo.enqueue(sample()).unwrap();
        assert_ne!(job.id.0, 0);
        assert!(repo.find_by_id(job.id).unwrap().is_some());
    }

    #[test]
    fn next_queued_orders_by_priority_then_fifo() {
        let repo = repo();
        let mut low = sample();
        low.priority = 0;
        let mut high = sample();
        high.priority = 5;

        let low = repo.enqueue(low).unwrap();
        let high = repo.enqueue(high).unwrap();

        let next = repo.next_queued().unwrap().unwrap();
        assert_eq!(next.id, high.id);
        let _ = low;
    }

    #[test]
    fn mark_running_then_succeeded_round_trips() {
        let repo = repo();
        let job = repo.enqueue(sample()).unwrap();
        let running = repo.mark_running(job.id).unwrap();
        assert_eq!(running.status, JobStatus::Running);
        assert!(running.started_at.is_some());

        let done = repo.mark_succeeded(job.id).unwrap();
        assert_eq!(done.status, JobStatus::Succeeded);
        assert!(done.completed_at.is_some());
    }

    #[test]
    fn mark_failed_requeues_within_retry_budget() {
        let repo = repo();
        let job = repo.enqueue(sample()).unwrap();
        repo.mark_running(job.id).unwrap();
        let failed_once = repo.mark_failed(job.id, "oops".to_string()).unwrap();
        assert_eq!(failed_once.status, JobStatus::Queued);
        assert_eq!(failed_once.retry_count, 1);
    }

    #[test]
    fn mark_failed_terminal_after_max_retries() {
        let repo = repo();
        let mut job_def = sample();
        job_def.max_retries = 1;
        let job = repo.enqueue(job_def).unwrap();

        repo.mark_running(job.id).unwrap();
        let first_fail = repo.mark_failed(job.id, "e1".to_string()).unwrap();
        assert_eq!(first_fail.status, JobStatus::Queued);

        repo.mark_running(job.id).unwrap();
        let second_fail = repo.mark_failed(job.id, "e2".to_string()).unwrap();
        assert_eq!(second_fail.status, JobStatus::Failed);
    }

    #[test]
    fn list_resumable_excludes_terminal_states() {
        let repo = repo();
        let queued = repo.enqueue(sample()).unwrap();
        let running = repo.enqueue(sample()).unwrap();
        repo.mark_running(running.id).unwrap();
        let succeeded = repo.enqueue(sample()).unwrap();
        repo.mark_running(succeeded.id).unwrap();
        repo.mark_succeeded(succeeded.id).unwrap();

        let resumable = repo.list_resumable().unwrap();
        let ids: Vec<_> = resumable.iter().map(|j| j.id).collect();
        assert!(ids.contains(&queued.id));
        assert!(ids.contains(&running.id));
        assert!(!ids.contains(&succeeded.id));
    }

    #[test]
    fn cancel_marks_cancelled() {
        let repo = repo();
        let job = repo.enqueue(sample()).unwrap();
        let cancelled = repo.cancel(job.id).unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
    }
}
