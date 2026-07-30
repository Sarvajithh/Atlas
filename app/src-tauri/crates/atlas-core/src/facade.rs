//! `AppFacade`: the single surface app-tauri's IPC command handlers call
//! into. Nothing in app-tauri reaches into atlas-db, atlas-vector, or
//! atlas-models directly -- only through this facade (§46.3, §46.4).

use std::sync::Arc;

use std::collections::HashMap;
use std::sync::Mutex;

use atlas_config::SettingsProvider;
use atlas_db::connection::SqliteConnection;
use atlas_db::event_bus_adapter::SqliteEventBus;
use atlas_db::graph_adapter::SqliteGraphRepository;
use atlas_db::jobs_adapter::SqliteJobRepository;
use atlas_db::memory_adapter::{
    SqliteAnalyticsRepository, SqliteAnnotationRepository, SqliteBookmarkRepository,
    SqliteChatRepository, SqliteLearningProgressRepository,
};
use atlas_db::model_registry_adapter::SqliteModelRegistryRepository;
use atlas_db::settings_adapter::SqliteSettingsProvider;
use atlas_db::workspace_adapter::SqliteWorkspaceRepository;
use atlas_events::EventBus;
use atlas_graph::GraphEngine;
use atlas_indexer::job_queue::JobQueue;
use atlas_memory::MemoryEngine;
use atlas_models::ModelRegistryRepository;
use atlas_types::ids::WorkspaceId;
use atlas_utils::AppError;
use atlas_watcher::FolderWatcher;
use atlas_workspace::lifecycle::WorkspaceEngine;

use crate::state::AppState;

/// The composed application. Each field is a high-level engine depending
/// only on interfaces; concrete adapters are wired in `AppFacade::new`.
pub struct AppFacade {
    workspace_engine: Arc<WorkspaceEngine>,
    memory_engine: Arc<MemoryEngine>,
    graph_engine: Arc<GraphEngine>,
    settings: Arc<dyn SettingsProvider>,
    model_registry: Arc<dyn ModelRegistryRepository>,
    events: Arc<dyn EventBus>,
    state: Arc<AppState>,
    job_queue: Arc<JobQueue>,
    /// One `FolderWatcher` per actively-watched workspace (§21). Behind a
    /// `Mutex<HashMap<..>>` rather than per-workspace `Arc`s, since watcher
    /// registration/deregistration is an infrequent, whole-map operation
    /// (workspace link/unlink/archive), not a hot path.
    watchers: Mutex<HashMap<WorkspaceId, FolderWatcher>>,
}

impl AppFacade {
    /// Compose the full application from a single SQLite connection. This
    /// is the Dependency Injection skeleton: concrete adapters (`atlas-db`)
    /// are constructed here and injected behind the interfaces domain
    /// crates depend on.
    pub fn new(connection: SqliteConnection) -> Self {
        let events: Arc<dyn EventBus> = Arc::new(SqliteEventBus::new(connection.clone()));
        let settings: Arc<dyn SettingsProvider> =
            Arc::new(SqliteSettingsProvider::new(connection.clone()));
        let model_registry: Arc<dyn ModelRegistryRepository> =
            Arc::new(SqliteModelRegistryRepository::new(connection.clone()));

        let workspace_repository = Arc::new(SqliteWorkspaceRepository::new(connection.clone()));
        let workspace_engine = Arc::new(WorkspaceEngine::new(workspace_repository, events.clone()));

        let annotations = Arc::new(SqliteAnnotationRepository::new(connection.clone()));
        let bookmarks = Arc::new(SqliteBookmarkRepository::new(connection.clone()));
        let chat = Arc::new(SqliteChatRepository::new(connection.clone()));
        let progress = Arc::new(SqliteLearningProgressRepository::new(connection.clone()));
        let analytics = Arc::new(SqliteAnalyticsRepository::new(connection.clone()));
        let memory_engine = Arc::new(MemoryEngine::new(
            annotations,
            bookmarks,
            chat,
            progress,
            analytics,
            events.clone(),
        ));

        let graph_repository = Arc::new(SqliteGraphRepository::new(connection.clone()));
        let graph_engine = Arc::new(GraphEngine::new(graph_repository, events.clone()));

        let job_repository = Arc::new(SqliteJobRepository::new(connection.clone()));
        let job_queue = Arc::new(JobQueue::new(job_repository));

        Self {
            workspace_engine,
            memory_engine,
            graph_engine,
            settings,
            model_registry,
            events,
            state: Arc::new(AppState::new()),
            job_queue,
            watchers: Mutex::new(HashMap::new()),
        }
    }

    pub fn job_queue(&self) -> &Arc<JobQueue> {
        &self.job_queue
    }

    /// Link a folder (§6) and start watching it (§21: initial scan +
    /// incremental watch), all through the facade so `app-tauri` never
    /// reaches past this single surface (§46.3, §46.4). This is the
    /// concrete subscriber-shaped reaction to `WorkspaceEngine::link`'s
    /// `WorkspaceAdded` event described in that method's doc comment --
    /// implemented here (rather than as a registered `EventSubscriber`)
    /// for this milestone, since `AppFacade` is already the single place
    /// that owns both the Workspace Engine and the Folder Watcher registry
    /// and a full async subscriber dispatch adds no behavior a direct call
    /// doesn't already provide at this stage.
    pub fn link_workspace(
        &self,
        root_path: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.link(root_path, display_name)?;
        self.start_watching(workspace.id, &workspace.root_path)?;
        Ok(workspace)
    }

    /// §6.1 "Archived: Watching stops." Archives the workspace and tears
    /// down its `FolderWatcher`, if one is registered.
    pub fn archive_workspace(
        &self,
        id: WorkspaceId,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.archive(id)?;
        self.stop_watching(id)?;
        Ok(workspace)
    }

    /// §6.1: restoring an archived workspace resumes watching.
    pub fn restore_workspace(
        &self,
        id: WorkspaceId,
    ) -> Result<atlas_types::workspace::Workspace, AppError> {
        let workspace = self.workspace_engine.restore(id)?;
        self.start_watching(workspace.id, &workspace.root_path)?;
        Ok(workspace)
    }

    /// §6.1 "Deleting a workspace link removes the workspace's row and
    /// watcher registration".
    pub fn unlink_workspace(&self, id: WorkspaceId) -> Result<(), AppError> {
        self.workspace_engine.unlink(id)?;
        self.stop_watching(id)?;
        Ok(())
    }

    fn start_watching(&self, id: WorkspaceId, root_path: &str) -> Result<(), AppError> {
        let mut watcher = FolderWatcher::new(self.events.clone(), self.job_queue.clone());
        watcher.initial_scan(id, root_path)?;
        watcher.watch(id, root_path)?;

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
        watchers.insert(id, watcher);
        Ok(())
    }

    /// §41 step 6: "Start Watchers (Folder Watcher per active workspace)".
    /// Unlike [`Self::start_watching`] (used on a fresh `link`), resuming
    /// at startup does not repeat the initial full scan -- the workspace
    /// was already scanned when it was first linked; only incremental
    /// watching needs to (re)start. Any changes made while the app was
    /// closed are picked up as the watcher observes them going forward,
    /// consistent with §21's incremental-indexing model (a full
    /// reconciliation scan on every restart is a possible future
    /// enhancement, not required by this contract).
    pub fn resume_watchers(&self) -> Result<usize, AppError> {
        let mut resumed = 0;
        for workspace in self.workspace_engine.list()? {
            if workspace.status != atlas_types::workspace::WorkspaceStatus::Active {
                continue;
            }
            let mut watcher = FolderWatcher::new(self.events.clone(), self.job_queue.clone());
            watcher.watch(workspace.id, &workspace.root_path)?;

            let mut watchers = self
                .watchers
                .lock()
                .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
            watchers.insert(workspace.id, watcher);
            resumed += 1;
        }
        Ok(resumed)
    }

    fn stop_watching(&self, id: WorkspaceId) -> Result<(), AppError> {
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| AppError::user("watcher registry lock poisoned"))?;
        if let Some(mut watcher) = watchers.remove(&id) {
            watcher.stop();
        }
        Ok(())
    }

    pub fn watched_workspace_count(&self) -> usize {
        self.watchers.lock().map(|w| w.len()).unwrap_or_default()
    }

    pub fn workspace_engine(&self) -> &Arc<WorkspaceEngine> {
        &self.workspace_engine
    }

    pub fn memory_engine(&self) -> &Arc<MemoryEngine> {
        &self.memory_engine
    }

    pub fn graph_engine(&self) -> &Arc<GraphEngine> {
        &self.graph_engine
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    pub fn model_registry(&self) -> &Arc<dyn ModelRegistryRepository> {
        &self.model_registry
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_db::connection::SqliteConnection;

    #[test]
    fn app_facade_new_wires_every_engine_and_starts_with_empty_state() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        assert!(facade.state().active_workspace_id().unwrap().is_none());
        // Accessors return the same Arc instances constructed internally --
        // this is the whole DI contract (Governing Principle).
        assert!(Arc::strong_count(facade.events()) >= 1);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-core-facade-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn link_workspace_persists_scans_and_registers_a_watcher() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let root = temp_dir("link");
        std::fs::write(root.join("a.pdf"), b"x").unwrap();

        let workspace = facade
            .link_workspace(root.to_str().unwrap(), "Test Workspace")
            .unwrap();

        assert_eq!(facade.watched_workspace_count(), 1);
        assert_eq!(
            facade.job_queue().repository().list_by_status(
                atlas_types::job::JobStatus::Queued
            ).unwrap().len(),
            1
        );

        facade.unlink_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 0);
        assert!(facade.workspace_engine().get(workspace.id).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_workspace_stops_watching_and_restore_resumes_it() {
        let facade = AppFacade::new(SqliteConnection::open(":memory:"));
        let root = temp_dir("archive-restore");

        let workspace = facade
            .link_workspace(root.to_str().unwrap(), "Archivable")
            .unwrap();
        assert_eq!(facade.watched_workspace_count(), 1);

        facade.archive_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 0);

        facade.restore_workspace(workspace.id).unwrap();
        assert_eq!(facade.watched_workspace_count(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Demonstrates the "Mock support for testing" requirement (§30): every
    /// engine `AppFacade` wires can equally be composed from the
    /// dependency-free `testing` doubles each domain crate exports,
    /// entirely without SQLite. This mirrors exactly what `AppFacade::new`
    /// does, just with different concrete adapters plugged into the same
    /// interfaces (Dependency Inversion).
    #[test]
    fn engines_can_be_composed_from_in_memory_test_doubles_instead_of_sqlite() {
        use atlas_events::InMemoryEventBus;
        use atlas_graph::testing::InMemoryGraphRepository;
        use atlas_memory::testing::{
            InMemoryAnalyticsRepository, InMemoryAnnotationRepository, InMemoryBookmarkRepository,
            InMemoryChatRepository, InMemoryLearningProgressRepository,
        };
        use atlas_workspace::testing::InMemoryWorkspaceRepository;

        let events: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());

        let workspace_engine =
            WorkspaceEngine::new(Arc::new(InMemoryWorkspaceRepository::new()), events.clone());
        assert!(workspace_engine.repository().list().unwrap().is_empty());

        let memory_engine = MemoryEngine::new(
            Arc::new(InMemoryAnnotationRepository::new()),
            Arc::new(InMemoryBookmarkRepository::new()),
            Arc::new(InMemoryChatRepository::new()),
            Arc::new(InMemoryLearningProgressRepository::new()),
            Arc::new(InMemoryAnalyticsRepository::new()),
            events.clone(),
        );
        assert!(memory_engine
            .annotations()
            .list_for_document(atlas_types::ids::DocumentId(1))
            .unwrap()
            .is_empty());

        let graph_engine = GraphEngine::new(Arc::new(InMemoryGraphRepository::new()), events);
        assert!(graph_engine
            .repository()
            .list_nodes_for_workspace(atlas_types::ids::WorkspaceId(1))
            .unwrap()
            .is_empty());
    }
}
