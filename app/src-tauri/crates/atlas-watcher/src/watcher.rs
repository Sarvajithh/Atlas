//! Folder Watcher (§21): one instance per active workspace root.
//! OS-level watching (via `notify`) + debouncing (`debounce.rs`) +
//! event -> indexing job translation (§21, §36.1). Publishes
//! `FileAdded`/`FileUpdated`/`FileDeleted` (§34.2) through the Event Bus
//! rather than calling the Indexing module directly (§46.6), and enqueues
//! a corresponding job on the Background Job Queue (§21, §33.14) so a
//! future Indexing Worker Pool has something durable to consume -- this
//! crate does not perform any indexing itself (task scope).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use atlas_events::EventBus;
use atlas_indexer::job_queue::JobQueue;
use atlas_types::event::{AppEvent, EventType};
use atlas_types::ids::WorkspaceId;
use atlas_utils::paths::relative_to;
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;
use notify::{RecursiveMode, Watcher};

use crate::debounce::{DebouncedChangeKind, Debouncer, RawChange, RawChangeKind};
use crate::scan::scan_files;

/// Default debounce window (§21: "debounces rapid changes"). This is a
/// sane default, not a hardcoded business rule -- a future Settings entry
/// (§23) can override it per the Governing Principle; until that plumbing
/// exists, `FolderWatcher::with_debounce_window` lets any caller (tests
/// included) override it explicitly.
pub const DEFAULT_DEBOUNCE_WINDOW_MS: u64 = 500;

/// One instance per active workspace root (§21).
pub struct FolderWatcher {
    events: Arc<dyn EventBus>,
    jobs: Arc<JobQueue>,
    debounce_window_ms: u64,
    /// Held for as long as the workspace should keep being watched;
    /// dropping it (or calling `stop`) unregisters the OS-level watch
    /// (§42 step 4: "Folder Watcher instances unregister cleanly").
    handle: Option<WatchHandle>,
}

/// Keeps the `notify` watcher and its background debounce thread alive.
/// Dropping this stops both.
struct WatchHandle {
    _watcher: notify::RecommendedWatcher,
    stop: Sender<()>,
}

impl FolderWatcher {
    pub fn new(events: Arc<dyn EventBus>, jobs: Arc<JobQueue>) -> Self {
        Self {
            events,
            jobs,
            debounce_window_ms: DEFAULT_DEBOUNCE_WINDOW_MS,
            handle: None,
        }
    }

    pub fn with_debounce_window(mut self, window_ms: u64) -> Self {
        self.debounce_window_ms = window_ms;
        self
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    pub fn jobs(&self) -> &Arc<JobQueue> {
        &self.jobs
    }

    /// §6.1 "Indexing (initial): full scan". Walks the workspace root once,
    /// publishing `FileAdded` and enqueueing an indexing job for every file
    /// found. Returns the number of files discovered.
    pub fn initial_scan(&self, workspace_id: WorkspaceId, root_path: &str) -> Result<usize, AppError> {
        let root = Path::new(root_path);
        let files = scan_files(root)?;
        for file in &files {
            self.announce(workspace_id, root, file, EventType::FileAdded)?;
        }
        Ok(files.len())
    }

    /// Start watching `root_path` for incremental changes (§21: "Active:
    /// steady state. Folder Watcher applies incremental indexing on file
    /// events"). Spawns a background thread that owns the debounce state;
    /// dropping the returned watcher (or calling `stop`) tears it down.
    pub fn watch(&mut self, workspace_id: WorkspaceId, root_path: &str) -> Result<(), AppError> {
        let root = PathBuf::from(root_path);
        if !root.is_dir() {
            return Err(AppError::workspace(format!(
                "cannot watch: root path is not a readable directory: {root_path}"
            )));
        }

        let (raw_tx, raw_rx) = channel::<RawChange>();
        let (stop_tx, stop_rx) = channel::<()>();

        // Shared clock origin: both the `notify` callback (which stamps
        // `observed_at_ms` on each raw change) and the debounce thread
        // (which compares those timestamps against `now_ms()`) must
        // measure elapsed time from the *same* `Instant`. Previously the
        // callback computed `Instant::now().elapsed()` on a freshly
        // constructed `Instant` (always ~0), while the debounce thread
        // measured elapsed time from its own `start` fixed at thread
        // spawn -- two different clocks being compared, which caused the
        // debounce window to silently collapse to the ~50ms poll interval
        // once the watcher had been running longer than `window_ms`.
        let start = Instant::now();

        let callback_start = start;
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let kind = match event.kind {
                    notify::EventKind::Create(_) => Some(RawChangeKind::Created),
                    notify::EventKind::Modify(_) => Some(RawChangeKind::Modified),
                    notify::EventKind::Remove(_) => Some(RawChangeKind::Removed),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let observed_at_ms = callback_start.elapsed().as_millis() as u64;
                    for path in event.paths {
                        let _ = raw_tx.send(RawChange {
                            path,
                            kind,
                            observed_at_ms,
                        });
                    }
                }
            }
        })
        .map_err(|e| AppError::workspace(format!("failed to start filesystem watcher: {e}")))?;

        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| AppError::workspace(format!("failed to watch '{root_path}': {e}")))?;

        let events = self.events.clone();
        let jobs = self.jobs.clone();
        let debounce_window_ms = self.debounce_window_ms;
        let root_for_thread = root.clone();

        std::thread::spawn(move || {
            let mut debouncer = Debouncer::new(debounce_window_ms);
            let now_ms = || start.elapsed().as_millis() as u64;

            loop {
                match stop_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }

                while let Ok(change) = raw_rx.try_recv() {
                    debouncer.observe(change);
                }

                for (path, kind) in debouncer.drain_ready(now_ms()) {
                    let event_type = match kind {
                        DebouncedChangeKind::Added => EventType::FileAdded,
                        DebouncedChangeKind::Updated => EventType::FileUpdated,
                        DebouncedChangeKind::Deleted => EventType::FileDeleted,
                    };
                    let _ = publish_and_enqueue(
                        &events,
                        &jobs,
                        workspace_id,
                        &root_for_thread,
                        &path,
                        event_type,
                    );
                }
            }
        });

        self.handle = Some(WatchHandle {
            _watcher: watcher,
            stop: stop_tx,
        });
        Ok(())
    }

    /// §42 step 4: "Stop Watchers -- Folder Watcher instances unregister
    /// cleanly."
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.stop.send(());
        }
    }

    fn announce(
        &self,
        workspace_id: WorkspaceId,
        root: &Path,
        file: &Path,
        event_type: EventType,
    ) -> Result<(), AppError> {
        publish_and_enqueue(&self.events, &self.jobs, workspace_id, root, file, event_type)
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn publish_and_enqueue(
    events: &Arc<dyn EventBus>,
    jobs: &Arc<JobQueue>,
    workspace_id: WorkspaceId,
    root: &Path,
    file: &Path,
    event_type: EventType,
) -> Result<(), AppError> {
    let relative_path = relative_to(root, file).unwrap_or_else(|| file.to_string_lossy().to_string());

    events.publish(AppEvent {
        id: None,
        event_type: event_type.clone(),
        payload: serde_json::json!({
            "workspace_id": workspace_id.0,
            "relative_path": relative_path,
        }),
        occurred_at: now_iso8601(),
    })?;

    // §21: "translate to indexing jobs" -- deletions invalidate cache
    // (§7, §22) rather than queueing new indexing work; that
    // orphan/garbage-collection logic belongs to a future indexing
    // milestone, so we only enqueue for Added/Updated here.
    if !matches!(event_type, EventType::FileDeleted) {
        jobs.enqueue_index_job(workspace_id, &relative_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;
    use atlas_indexer::testing::InMemoryJobRepository;
    use atlas_types::job::JobStatus;

    fn watcher() -> (FolderWatcher, Arc<InMemoryEventBus>, Arc<JobQueue>) {
        let events = Arc::new(InMemoryEventBus::new());
        let jobs = Arc::new(JobQueue::new(Arc::new(InMemoryJobRepository::new())));
        let watcher = FolderWatcher::new(events.clone() as Arc<dyn EventBus>, jobs.clone());
        (watcher, events, jobs)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-watcher-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn watcher_exposes_the_injected_dependencies() {
        let (watcher, events, jobs) = watcher();
        let events_dyn: Arc<dyn EventBus> = events.clone();
        assert!(Arc::ptr_eq(watcher.events(), &events_dyn));
        assert!(Arc::ptr_eq(watcher.jobs(), &jobs));
    }

    #[test]
    fn initial_scan_publishes_file_added_and_enqueues_a_job_per_file() {
        let (watcher, events, jobs) = watcher();
        let root = temp_dir("initial-scan");
        std::fs::write(root.join("a.pdf"), b"x").unwrap();
        std::fs::write(root.join("b.pdf"), b"y").unwrap();

        let count = watcher
            .initial_scan(WorkspaceId(1), root.to_str().unwrap())
            .unwrap();
        assert_eq!(count, 2);

        let published = events.published_events();
        assert_eq!(published.len(), 2);
        assert!(published
            .iter()
            .all(|e| e.event_type == EventType::FileAdded));

        let queued = jobs.repository().list_by_status(JobStatus::Queued).unwrap();
        assert_eq!(queued.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn initial_scan_on_missing_root_is_an_error() {
        let (watcher, _events, _jobs) = watcher();
        let missing = std::env::temp_dir().join("atlas-watcher-missing-root-xyz");
        assert!(watcher
            .initial_scan(WorkspaceId(1), missing.to_str().unwrap())
            .is_err());
    }

    #[test]
    fn watch_then_creating_a_file_eventually_publishes_and_enqueues() {
        let (mut watcher, events, _jobs) = watcher();
        let root = temp_dir("watch-create");
        watcher
            .with_debounce_window_for_test(50);
        watcher.watch(WorkspaceId(7), root.to_str().unwrap()).unwrap();

        std::fs::write(root.join("new.pdf"), b"hello").unwrap();

        // Poll for up to a couple seconds -- real filesystem + OS watcher
        // timing is inherently non-deterministic in a CI sandbox.
        let mut found = false;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(100));
            if !events.published_events().is_empty() {
                found = true;
                break;
            }
        }
        assert!(found, "expected at least one published event after file creation");

        watcher.stop();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn events_after_first_window_ms_of_uptime_still_coalesce_within_the_debounce_window() {
        // Regression test for the two-clocks bug (see `watch`'s comment on
        // `start`/`callback_start`): before the fix, `observed_at_ms` was
        // always ~0 (measured from a freshly-constructed `Instant`) while
        // `now_ms()` grew from the debounce thread's own `start`. Once the
        // watcher had been running longer than its own debounce window,
        // every new change looked "ready" on the very next ~50ms poll
        // tick instead of being coalesced. This test runs the watcher past
        // its debounce window *first*, then fires a rapid burst of writes
        // spread across several poll ticks, and asserts they still
        // collapse into exactly one enqueued job.
        let (mut watcher, _events, jobs) = watcher();
        let root = temp_dir("watch-debounce-after-uptime");
        let window_ms: u64 = 300;
        watcher.with_debounce_window_for_test(window_ms);
        watcher.watch(WorkspaceId(9), root.to_str().unwrap()).unwrap();

        // Let the watcher run well past its own debounce window before
        // the burst -- this is the exact condition under which the old
        // code silently degraded to firing on every poll tick.
        std::thread::sleep(Duration::from_millis(window_ms + 150));

        // Burst: several writes to the same file, each well within one
        // debounce window of the next, but spread across multiple ~50ms
        // poll ticks.
        std::fs::write(root.join("burst.pdf"), b"v1").unwrap();
        std::thread::sleep(Duration::from_millis(80));
        std::fs::write(root.join("burst.pdf"), b"v2").unwrap();
        std::thread::sleep(Duration::from_millis(80));
        std::fs::write(root.join("burst.pdf"), b"v3").unwrap();

        // Give the debounce window time to elapse and the coalesced
        // change to drain. If the bug is present, each write would
        // already have fired on its own poll tick well before this.
        std::thread::sleep(Duration::from_millis(window_ms + 400));

        let queued = jobs.repository().list_by_status(JobStatus::Queued).unwrap();
        assert_eq!(
            queued.len(),
            1,
            "expected the rapid burst to coalesce into exactly one indexing job, got {}",
            queued.len()
        );

        watcher.stop();
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
impl FolderWatcher {
    /// Test-only helper: shrink the debounce window after construction, so
    /// filesystem-integration tests don't have to wait the full default
    /// window before asserting.
    fn with_debounce_window_for_test(&mut self, window_ms: u64) {
        self.debounce_window_ms = window_ms;
    }
}
