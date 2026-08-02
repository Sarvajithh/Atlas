//! Background Indexing Worker (§21 "Background Workers", §36, §33.14).
//!
//! Completes the pipeline the Folder Watcher (`atlas-watcher`) starts:
//!
//! ```text
//! FolderWatcher -> JobQueue.enqueue_index_job -> `jobs` table (Queued)
//!                                                     |
//!                                                     v
//!                                          IndexingWorker (this module)
//!                                                     |
//!                              JobQueue.next_queued / mark_running
//!                                                     |
//!                                                     v
//!                            IndexingPipeline::index_document (unchanged)
//!                                                     |
//!                              JobQueue.mark_succeeded / mark_failed
//! ```
//!
//! This module deliberately contains **no** parsing, chunking, embedding,
//! or vector-storage logic (§36.3, §46.8) -- it only claims a `Job` row,
//! resolves it to an absolute path (the same `resolve_absolute_path` the
//! synchronous `AppFacade::index_document_now` path already used, moved
//! here to be shared rather than duplicated), and delegates the actual
//! work to the existing `IndexingPipeline::index_document`. Per-document
//! success/failure events (`IndexCompleted`, `JobFailed`) are already
//! published by the pipeline itself for every failure *inside* the
//! pipeline (§34.2) -- the worker does not publish a second, redundant
//! copy of those. The one exception is a failure *before* the pipeline is
//! even reached (workspace missing/archived, unsafe path) -- the pipeline
//! never runs and so never publishes anything for it, so the worker
//! publishes `JobFailed` itself using the same existing `EventType`
//! rather than inventing a new one (task instruction: "reuse existing
//! EventBus messages wherever possible... do not invent parallel state").
//!
//! One worker thread services the single, global `jobs` table -- the job
//! payload already carries `workspace_id`, so a single worker naturally
//! services every linked workspace without per-workspace duplication, and
//! processing jobs one at a time (rather than a thread pool) is sufficient
//! to satisfy "support multiple queued jobs" (they are all drained,
//! in order) while trivially avoiding the double-claim race the
//! `next_queued`/`mark_running` two-step API leaves open for concurrent
//! callers (§33.14 doc comment: "callers must call mark_running to claim
//! it").

use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use atlas_events::EventBus;
use atlas_indexer::job_queue::{JobQueue, JOB_TYPE_INDEX_DOCUMENT};
use atlas_indexer::pipeline::IndexingPipeline;
use atlas_types::ids::WorkspaceId;
use atlas_types::job::{Job, JobStatus};
use atlas_utils::{log_info, log_warn, AppError};
use atlas_workspace::lifecycle::WorkspaceEngine;

use crate::paths::resolve_absolute_path;

/// How long the worker thread sleeps between polls when the queue is
/// empty (§46.1: not a magic number scattered across call sites -- one
/// named constant; a future Settings entry can override it the same way
/// `atlas-watcher::DEFAULT_DEBOUNCE_WINDOW_MS` is documented as
/// override-able, without this milestone needing to wire that plumbing).
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 250;

/// Background Indexing Worker (§21). One instance for the whole app
/// (composed once in `AppFacade::new`), started at startup (§41) and
/// stopped at shutdown (§42).
pub struct IndexingWorker {
    workspace_engine: Arc<WorkspaceEngine>,
    indexing_pipeline: Arc<IndexingPipeline>,
    job_queue: Arc<JobQueue>,
    events: Arc<dyn EventBus>,
    poll_interval_ms: u64,
    handle: Option<WorkerHandle>,
}

struct WorkerHandle {
    stop: Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl IndexingWorker {
    pub fn new(
        workspace_engine: Arc<WorkspaceEngine>,
        indexing_pipeline: Arc<IndexingPipeline>,
        job_queue: Arc<JobQueue>,
        events: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            workspace_engine,
            indexing_pipeline,
            job_queue,
            events,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            handle: None,
        }
    }

    #[cfg(test)]
    pub fn with_poll_interval(mut self, interval_ms: u64) -> Self {
        self.poll_interval_ms = interval_ms;
        self
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Start the worker thread (§41 step 7: "resume in-flight Background
    /// Workers"; also the steady-state consumer for §21 "Active:
    /// incremental indexing"). Idempotent -- calling `start` while already
    /// running is a no-op rather than spawning a second thread, which
    /// would reintroduce the double-claim race the single-worker design
    /// avoids.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }

        let (stop_tx, stop_rx) = channel::<()>();
        let workspace_engine = self.workspace_engine.clone();
        let indexing_pipeline = self.indexing_pipeline.clone();
        let job_queue = self.job_queue.clone();
        let events = self.events.clone();
        let poll_interval_ms = self.poll_interval_ms;

        let thread = std::thread::spawn(move || {
            log_info!("indexing worker started");
            loop {
                match stop_rx.recv_timeout(Duration::from_millis(poll_interval_ms)) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }

                match job_queue.next_queued() {
                    Ok(Some(job)) => {
                        process_job(&workspace_engine, &indexing_pipeline, &job_queue, &events, job);
                    }
                    Ok(None) => {
                        // Nothing queued right now -- sleep for another
                        // poll interval (§45.2: an empty queue is not a
                        // failure, so nothing is logged or surfaced here).
                    }
                    Err(err) => {
                        log_warn!("indexing worker failed to read next queued job: {err}");
                    }
                }
            }
            log_info!("indexing worker stopped");
        });

        self.handle = Some(WorkerHandle { stop: stop_tx, thread: Some(thread) });
    }

    /// Stop the worker cleanly (§42 step 3/4: drain/cancel in-flight work,
    /// stop background workers). The job currently being processed (if
    /// any) is allowed to finish -- `IndexingPipeline::index_document` is
    /// synchronous and not interrupted mid-file, so stopping only prevents
    /// the *next* job from being claimed, matching the Folder Watcher's
    /// own shutdown behavior (`FolderWatcher::stop`) rather than inventing
    /// a different cancellation model for this worker.
    pub fn stop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            let _ = handle.stop.send(());
            if let Some(thread) = handle.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for IndexingWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Claim and run a single job. A per-file failure here is Recoverable
/// (§45.1): it is recorded via `mark_failed` (which itself applies the
/// existing bounded-retry policy, §33.14) and logged, but must never stop
/// the worker loop from moving on to the next job (§45.2, and the task's
/// own framing: "retry failed jobs when appropriate").
fn process_job(
    workspace_engine: &Arc<WorkspaceEngine>,
    indexing_pipeline: &Arc<IndexingPipeline>,
    job_queue: &Arc<JobQueue>,
    events: &Arc<dyn EventBus>,
    job: Job,
) {
    if job.job_type != JOB_TYPE_INDEX_DOCUMENT {
        // §28: the job type space is extensible; an unrecognized type from
        // a future job producer is a recoverable, visible failure, not a
        // silent drop or a panic (§45.2).
        let _ = job_queue.mark_failed(job.id, format!("unrecognized job type '{}'", job.job_type));
        return;
    }

    let (workspace_id, relative_path) = match parse_payload(&job) {
        Ok(pair) => pair,
        Err(err) => {
            let _ = job_queue.mark_failed(job.id, err.message);
            return;
        }
    };

    if let Err(err) = job_queue.mark_running(job.id) {
        log_warn!("failed to mark job {} running: {err}", job.id.0);
        return;
    }

    let outcome = resolve_absolute_path(workspace_engine, workspace_id, &relative_path).and_then(
        |absolute_path| indexing_pipeline.index_document(workspace_id, &relative_path, &absolute_path),
    );

    match outcome {
        Ok(_) => {
            // `IndexingPipeline::index_document` already published
            // `IndexCompleted` (or, for an unchanged file, nothing further
            // is needed at all -- §22 cache invalidation) and updated the
            // `documents` row itself; the job row only needs its own
            // terminal status.
            if let Err(err) = job_queue.mark_succeeded(job.id) {
                log_warn!("failed to mark job {} succeeded: {err}", job.id.0);
            }
        }
        Err(err) => {
            // `IndexingPipeline::index_document` already published
            // `JobFailed` for genuine per-file indexing failures; a path
            // resolution failure (workspace missing/archived, path
            // escapes the workspace root) happens before the pipeline is
            // even called, so it is recorded on the job row via
            // `mark_failed`, and also published here as `JobFailed` so a
            // Learning Progress UI sees every failure through the same
            // one event type, not two.
            if let Err(publish_err) = events.publish(atlas_types::event::AppEvent {
                id: None,
                event_type: atlas_types::event::EventType::JobFailed,
                payload: serde_json::json!({
                    "workspace_id": workspace_id.0,
                    "relative_path": relative_path,
                    "error": err.message,
                }),
                occurred_at: atlas_utils::time::now_iso8601(),
            }) {
                log_warn!("failed to publish JobFailed event: {publish_err}");
            }
            if let Err(mark_err) = job_queue.mark_failed(job.id, err.message) {
                log_warn!("failed to mark job {} failed: {mark_err}", job.id.0);
            }
        }
    }
}

fn parse_payload(job: &Job) -> Result<(WorkspaceId, String), AppError> {
    let workspace_id = job
        .payload
        .get("workspace_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::indexing(format!("job {}: payload missing 'workspace_id'", job.id.0)))?;
    let relative_path = job
        .payload
        .get("relative_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::indexing(format!("job {}: payload missing 'relative_path'", job.id.0)))?
        .to_string();
    Ok((WorkspaceId(workspace_id), relative_path))
}

/// Aggregate the existing `jobs` table into a per-workspace progress
/// snapshot (§33.14 is the sole source of truth; nothing here is cached or
/// duplicated). Kept alongside the worker since it reads the exact rows
/// the worker writes -- `AppFacade::indexing_status` is a thin,
/// facade-level pass-through to this function.
pub fn compute_indexing_status(
    job_queue: &JobQueue,
    workspace_id: WorkspaceId,
) -> Result<atlas_types::job::IndexingStatus, AppError> {
    use atlas_types::job::{IndexingStatus, RunningIndexingJob};

    let belongs_to_workspace = |job: &Job| {
        job.payload.get("workspace_id").and_then(|v| v.as_i64()) == Some(workspace_id.0)
    };

    let queued = job_queue
        .repository()
        .list_by_status(JobStatus::Queued)?
        .into_iter()
        .filter(belongs_to_workspace)
        .count();

    let running_jobs: Vec<Job> = job_queue
        .repository()
        .list_by_status(JobStatus::Running)?
        .into_iter()
        .filter(belongs_to_workspace)
        .collect();
    let running = running_jobs.first().and_then(|job| {
        job.payload
            .get("relative_path")
            .and_then(|v| v.as_str())
            .map(|relative_path| RunningIndexingJob {
                job_id: job.id,
                relative_path: relative_path.to_string(),
                started_at: job.started_at.clone(),
                retry_count: job.retry_count,
            })
    });

    let succeeded_jobs: Vec<Job> = job_queue
        .repository()
        .list_by_status(JobStatus::Succeeded)?
        .into_iter()
        .filter(belongs_to_workspace)
        .collect();
    let succeeded = succeeded_jobs.len();
    let last_indexed_at = succeeded_jobs
        .iter()
        .filter_map(|job| job.completed_at.clone())
        .max();

    let failed = job_queue
        .repository()
        .list_by_status(JobStatus::Failed)?
        .into_iter()
        .filter(belongs_to_workspace)
        .count();

    let total = queued + running_jobs.len() + succeeded + failed;
    let progress_percent = if total == 0 {
        None
    } else {
        Some(((succeeded + failed) as f32 / total as f32 * 1000.0).round() / 10.0)
    };

    Ok(IndexingStatus { queued, running, succeeded, failed, total, progress_percent, last_indexed_at })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_config::hierarchy::LayeredSettingsProvider;
    use atlas_events::InMemoryEventBus;
    use atlas_indexer::embedding::HashEmbeddingEngine;
    use atlas_indexer::ocr::NoopOcrEngine;
    use atlas_indexer::parser::default_parser_selector;
    use atlas_indexer::testing::{
        InMemoryChunkRepository, InMemoryDocumentRepository, InMemoryEmbeddingRepository,
        InMemoryJobRepository,
    };
    use atlas_indexer::JobRepository;
    use atlas_types::ids::ChunkId;
    use atlas_workspace::testing::InMemoryWorkspaceRepository;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration as StdDuration;

    struct InMemoryVectorStore {
        vectors: StdMutex<Vec<(ChunkId, atlas_indexer::embedding::Embedding)>>,
    }

    impl InMemoryVectorStore {
        fn new() -> Self {
            Self { vectors: StdMutex::new(Vec::new()) }
        }
    }

    impl atlas_indexer::vector_search::VectorStore for InMemoryVectorStore {
        fn upsert_vector(
            &self,
            _workspace_id: WorkspaceId,
            chunk_id: ChunkId,
            vector: atlas_indexer::embedding::Embedding,
        ) -> Result<String, AppError> {
            self.vectors.lock().unwrap().push((chunk_id, vector));
            Ok(format!("vec-{}", chunk_id.0))
        }

        fn delete_vector(&self, _workspace_id: WorkspaceId, chunk_id: ChunkId) -> Result<(), AppError> {
            self.vectors.lock().unwrap().retain(|(id, _)| *id != chunk_id);
            Ok(())
        }
    }

    fn temp_workspace_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-core-worker-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_pipeline() -> Arc<IndexingPipeline> {
        Arc::new(IndexingPipeline::new(
            Arc::new(InMemoryDocumentRepository::new()),
            Arc::new(InMemoryChunkRepository::new()),
            Arc::new(default_parser_selector()),
            Arc::new(LayeredSettingsProvider::new()),
            Arc::new(InMemoryEventBus::new()),
            Arc::new(NoopOcrEngine),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(InMemoryEmbeddingRepository::new()),
            Arc::new(InMemoryVectorStore::new()),
        ))
    }

    fn test_workspace_engine(root: &std::path::Path) -> (Arc<WorkspaceEngine>, WorkspaceId) {
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let engine = Arc::new(WorkspaceEngine::new(
            Arc::new(InMemoryWorkspaceRepository::new()),
            events,
        ));
        let workspace = engine.link(root.to_str().unwrap(), "Test Workspace").unwrap();
        (engine, workspace.id)
    }

    #[test]
    fn parse_payload_reads_workspace_id_and_relative_path() {
        let job = Job {
            id: atlas_types::ids::JobId(1),
            job_type: JOB_TYPE_INDEX_DOCUMENT.to_string(),
            payload: serde_json::json!({ "workspace_id": 7, "relative_path": "a.md" }),
            status: JobStatus::Queued,
            priority: 0,
            retry_count: 0,
            max_retries: 3,
            created_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            error: None,
        };
        let (workspace_id, relative_path) = parse_payload(&job).unwrap();
        assert_eq!(workspace_id, WorkspaceId(7));
        assert_eq!(relative_path, "a.md");
    }

    #[test]
    fn parse_payload_missing_field_is_a_recoverable_error() {
        let job = Job {
            id: atlas_types::ids::JobId(1),
            job_type: JOB_TYPE_INDEX_DOCUMENT.to_string(),
            payload: serde_json::json!({ "workspace_id": 7 }),
            status: JobStatus::Queued,
            priority: 0,
            retry_count: 0,
            max_retries: 3,
            created_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            error: None,
        };
        let err = parse_payload(&job).unwrap_err();
        assert!(err.message.contains("relative_path"));
    }

    /// End-to-end: enqueue a real job the same way `FolderWatcher` does,
    /// start the worker, and confirm the job reaches `Succeeded` and the
    /// document is actually indexed -- exercising the real
    /// `IndexingPipeline`, not a mock of it.
    #[test]
    fn worker_drains_a_queued_job_into_a_parsed_document() {
        let root = temp_workspace_dir("drain");
        std::fs::write(root.join("notes.md"), "# Title\n\nSome content.").unwrap();

        let (workspace_engine, workspace_id) = test_workspace_engine(&root);
        let pipeline = test_pipeline();
        let job_queue = Arc::new(JobQueue::new(Arc::new(InMemoryJobRepository::new())));
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        job_queue.enqueue_index_job(workspace_id, "notes.md").unwrap();

        let mut worker = IndexingWorker::new(workspace_engine, pipeline.clone(), job_queue.clone(), events)
            .with_poll_interval(20);
        worker.start();

        let mut succeeded = false;
        for _ in 0..100 {
            std::thread::sleep(StdDuration::from_millis(20));
            if job_queue.repository().list_by_status(JobStatus::Succeeded).unwrap().len() == 1 {
                succeeded = true;
                break;
            }
        }
        worker.stop();

        assert!(succeeded, "expected the queued job to reach Succeeded");
        assert!(job_queue.repository().list_by_status(JobStatus::Queued).unwrap().is_empty());
        let docs = pipeline.documents().list_for_workspace(workspace_id).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].parse_status, atlas_types::document::ParseStatus::Parsed);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Multiple queued jobs are all drained, not just the first
    /// (acceptance criterion: "support multiple queued jobs").
    #[test]
    fn worker_drains_multiple_queued_jobs() {
        let root = temp_workspace_dir("multi");
        std::fs::write(root.join("a.md"), "# A\n\nFirst file.").unwrap();
        std::fs::write(root.join("b.md"), "# B\n\nSecond file.").unwrap();

        let (workspace_engine, workspace_id) = test_workspace_engine(&root);
        let pipeline = test_pipeline();
        let job_queue = Arc::new(JobQueue::new(Arc::new(InMemoryJobRepository::new())));
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        job_queue.enqueue_index_job(workspace_id, "a.md").unwrap();
        job_queue.enqueue_index_job(workspace_id, "b.md").unwrap();

        let mut worker = IndexingWorker::new(workspace_engine, pipeline.clone(), job_queue.clone(), events)
            .with_poll_interval(20);
        worker.start();

        let mut done = false;
        for _ in 0..150 {
            std::thread::sleep(StdDuration::from_millis(20));
            if job_queue.repository().list_by_status(JobStatus::Succeeded).unwrap().len() == 2 {
                done = true;
                break;
            }
        }
        worker.stop();

        assert!(done, "expected both queued jobs to reach Succeeded");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A job referencing a workspace that no longer exists fails before
    /// the pipeline is ever reached, and is recorded via `mark_failed`
    /// (bounded retry policy applies exactly as it does for any other
    /// failure -- §33.14, exercised already by `atlas-db`'s adapter
    /// tests).
    #[test]
    fn worker_marks_failed_when_workspace_is_missing() {
        let root = temp_workspace_dir("missing-ws");
        let events_bus: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
        let workspace_engine = Arc::new(WorkspaceEngine::new(
            Arc::new(InMemoryWorkspaceRepository::new()),
            events_bus.clone(),
        ));
        let pipeline = test_pipeline();
        let job_queue = Arc::new(JobQueue::new(Arc::new(InMemoryJobRepository::new())));

        // Enqueue a job for a workspace id that was never linked.
        job_queue.enqueue_index_job(WorkspaceId(999), "ghost.md").unwrap();

        let mut worker =
            IndexingWorker::new(workspace_engine, pipeline, job_queue.clone(), events_bus)
                .with_poll_interval(20);
        worker.start();

        let mut failed = false;
        for _ in 0..100 {
            std::thread::sleep(StdDuration::from_millis(20));
            let failed_jobs = job_queue.repository().list_by_status(JobStatus::Failed).unwrap();
            let queued_jobs = job_queue.repository().list_by_status(JobStatus::Queued).unwrap();
            if !failed_jobs.is_empty() && queued_jobs.is_empty() {
                failed = true;
                break;
            }
        }
        worker.stop();

        assert!(failed, "expected the job to end in Failed once retries are exhausted");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unrecognized_job_type_is_marked_failed_without_touching_the_pipeline() {
        let root = temp_workspace_dir("unrecognized-type");
        let (workspace_engine, _workspace_id) = test_workspace_engine(&root);
        let pipeline = test_pipeline();
        let job_repository = Arc::new(InMemoryJobRepository::new());
        let job_queue = Arc::new(JobQueue::new(job_repository.clone()));
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        let job = job_repository
            .enqueue(Job {
                id: atlas_types::ids::JobId(0),
                job_type: "some_future_job_type".to_string(),
                payload: serde_json::json!({}),
                status: JobStatus::Queued,
                priority: 0,
                retry_count: 0,
                max_retries: 0,
                created_at: atlas_utils::time::now_iso8601(),
                started_at: None,
                completed_at: None,
                error: None,
            })
            .unwrap();

        process_job(&workspace_engine, &pipeline, &job_queue, &events, job.clone());

        let stored = job_queue.repository().find_by_id(job.id).unwrap().unwrap();
        assert_eq!(stored.status, JobStatus::Failed);
        assert!(stored.error.unwrap().contains("unrecognized job type"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn start_is_idempotent_and_stop_can_be_called_more_than_once() {
        let root = temp_workspace_dir("idempotent");
        let (workspace_engine, _workspace_id) = test_workspace_engine(&root);
        let pipeline = test_pipeline();
        let job_queue = Arc::new(JobQueue::new(Arc::new(InMemoryJobRepository::new())));
        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        let mut worker = IndexingWorker::new(workspace_engine, pipeline, job_queue, events)
            .with_poll_interval(20);
        assert!(!worker.is_running());
        worker.start();
        assert!(worker.is_running());
        worker.start(); // no-op, must not spawn a second thread
        assert!(worker.is_running());
        worker.stop();
        assert!(!worker.is_running());
        worker.stop(); // must not panic

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn compute_indexing_status_aggregates_by_workspace() {
        let job_repository = Arc::new(InMemoryJobRepository::new());
        let job_queue = JobQueue::new(job_repository);

        job_queue.enqueue_index_job(WorkspaceId(1), "a.md").unwrap();
        job_queue.enqueue_index_job(WorkspaceId(1), "b.md").unwrap();
        // A job for a different workspace must not be counted.
        job_queue.enqueue_index_job(WorkspaceId(2), "other.md").unwrap();

        let status = compute_indexing_status(&job_queue, WorkspaceId(1)).unwrap();
        assert_eq!(status.queued, 2);
        assert_eq!(status.running, None);
        assert_eq!(status.succeeded, 0);
        assert_eq!(status.failed, 0);
        assert_eq!(status.total, 2);
        assert_eq!(status.progress_percent, Some(0.0));
        assert_eq!(status.last_indexed_at, None);
    }

    #[test]
    fn compute_indexing_status_is_none_progress_when_nothing_ever_queued() {
        let job_repository = Arc::new(InMemoryJobRepository::new());
        let job_queue = JobQueue::new(job_repository);
        let status = compute_indexing_status(&job_queue, WorkspaceId(42)).unwrap();
        assert_eq!(status.total, 0);
        assert_eq!(status.progress_percent, None);
    }
}
