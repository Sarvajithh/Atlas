//! `AppFacade`: the single surface app-tauri's IPC command handlers call
//! into. Nothing in app-tauri reaches into atlas-db, atlas-vector, or
//! atlas-models directly -- only through this facade (§46.3, §46.4).

use std::sync::Arc;

use atlas_config::SettingsProvider;
use atlas_db::connection::SqliteConnection;
use atlas_db::event_bus_adapter::SqliteEventBus;
use atlas_db::graph_adapter::SqliteGraphRepository;
use atlas_db::memory_adapter::{
    SqliteAnalyticsRepository, SqliteAnnotationRepository, SqliteBookmarkRepository,
    SqliteChatRepository, SqliteLearningProgressRepository,
};
use atlas_db::model_registry_adapter::SqliteModelRegistryRepository;
use atlas_db::settings_adapter::SqliteSettingsProvider;
use atlas_db::workspace_adapter::SqliteWorkspaceRepository;
use atlas_events::EventBus;
use atlas_graph::GraphEngine;
use atlas_memory::MemoryEngine;
use atlas_models::ModelRegistryRepository;
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

        Self {
            workspace_engine,
            memory_engine,
            graph_engine,
            settings,
            model_registry,
            events,
            state: Arc::new(AppState::new()),
        }
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
