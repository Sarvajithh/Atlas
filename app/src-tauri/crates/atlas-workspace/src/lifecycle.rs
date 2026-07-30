//! Workspace lifecycle transitions (§6.1):
//! Unlinked -> Linking -> Indexing (initial) -> Active -> Archived -> (Unlinked)
//!
//! `WorkspaceEngine` is the only component that mutates a workspace's
//! status (§46.2: single owner per responsibility). It depends only on
//! `WorkspaceRepository` and `EventBus` (Dependency Inversion); concrete
//! SQLite storage and the real Folder Watcher are wired in by atlas-core,
//! which reacts to the `WorkspaceAdded`/`WorkspaceRemoved` events this
//! engine publishes rather than being called directly (§46.6).

use std::sync::Arc;

use atlas_events::EventBus;
use atlas_types::event::{AppEvent, EventType};
use atlas_types::ids::WorkspaceId;
use atlas_types::workspace::{Workspace, WorkspaceStatus};
use atlas_utils::time::now_iso8601;
use atlas_utils::validation::require_non_empty;
use atlas_utils::AppError;

use crate::repository::WorkspaceRepository;

/// High-level module depending only on interfaces (Governing Principle,
/// Dependency Inversion). Concrete repository/event-bus implementations are
/// injected by atlas-core at composition time.
pub struct WorkspaceEngine {
    repository: Arc<dyn WorkspaceRepository>,
    events: Arc<dyn EventBus>,
}

impl WorkspaceEngine {
    pub fn new(repository: Arc<dyn WorkspaceRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repository, events }
    }

    /// Access to the injected repository, for future lifecycle methods.
    pub fn repository(&self) -> &Arc<dyn WorkspaceRepository> {
        &self.repository
    }

    /// Access to the injected event bus, for future lifecycle methods.
    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    /// Link a folder from the filesystem (§6: "Users never upload files.
    /// They link folders."). Validates the path is a non-empty, readable
    /// directory, then persists a new workspace row.
    ///
    /// Lifecycle (§6.1): a newly linked workspace starts `Linking`. This
    /// milestone does not implement the OCR/embedding indexing pipeline
    /// (out of scope, per the task), so there is nothing that would ever
    /// advance a workspace out of `Indexing` on its own; rather than leave
    /// every new workspace stuck in an unreachable state, `link` persists
    /// it as `Linking` and immediately transitions it to `Active` once the
    /// row exists, mirroring what "Active: steady state, watcher applies
    /// incremental indexing" means when there is no initial indexing
    /// pipeline yet to wait on. A `WorkspaceAdded` event (§34.2) is
    /// published so the Watcher Module and a future Indexing Module can
    /// react (§46.6: engines never call each other directly when an event
    /// exists) -- e.g. by performing the initial scan (§6.1) and starting
    /// incremental watching.
    pub fn link(&self, root_path: impl Into<String>, display_name: impl Into<String>) -> Result<Workspace, AppError> {
        let root_path = root_path.into();
        let display_name = display_name.into();
        require_non_empty("root_path", &root_path)?;
        require_non_empty("display_name", &display_name)?;
        validate_root_path(&root_path)?;

        let created_at = now_iso8601();
        let inserted = self.repository.insert(Workspace {
            id: WorkspaceId(0),
            root_path,
            display_name,
            status: WorkspaceStatus::Linking,
            created_at: created_at.clone(),
            last_indexed_at: None,
        })?;

        let activated = self.repository.update(Workspace {
            status: WorkspaceStatus::Active,
            ..inserted.clone()
        })?;

        self.events.publish(AppEvent {
            id: None,
            event_type: EventType::WorkspaceAdded,
            payload: serde_json::json!({
                "workspace_id": activated.id.0,
                "root_path": activated.root_path,
            }),
            occurred_at: now_iso8601(),
        })?;

        Ok(activated)
    }

    pub fn get(&self, id: WorkspaceId) -> Result<Option<Workspace>, AppError> {
        self.repository.find_by_id(id)
    }

    pub fn list(&self) -> Result<Vec<Workspace>, AppError> {
        self.repository.list()
    }

    /// Rename a workspace's display name (§8.2.1, §33.1's `display_name`).
    /// Never touches `root_path` -- renaming is purely cosmetic and must
    /// not be confused with re-linking a different folder.
    pub fn rename(&self, id: WorkspaceId, new_display_name: impl Into<String>) -> Result<Workspace, AppError> {
        let new_display_name = new_display_name.into();
        require_non_empty("display_name", &new_display_name)?;

        let workspace = self.require_workspace(id)?;
        self.repository.update(Workspace {
            display_name: new_display_name,
            ..workspace
        })
    }

    /// Archive a workspace (§6.1: "Watching stops. Derived data is
    /// retained and queryable, but no new indexing happens."). Archiving
    /// an already-archived workspace is a no-op success, not an error --
    /// idempotent per §34.3's expectation that state transitions tolerate
    /// redelivery.
    pub fn archive(&self, id: WorkspaceId) -> Result<Workspace, AppError> {
        let workspace = self.require_workspace(id)?;
        if workspace.status == WorkspaceStatus::Archived {
            return Ok(workspace);
        }
        self.repository.update(Workspace {
            status: WorkspaceStatus::Archived,
            ..workspace
        })
    }

    /// Restore an archived workspace back to `Active` (§6.1: "Archived ->
    /// (optionally) Unlinked" describes one exit; restoring to Active is
    /// the other, implied by "Deleting a workspace link... is a separate,
    /// explicit action" from archiving). Only archived workspaces can be
    /// restored -- restoring a workspace that was never archived is a
    /// user error, since the caller's mental model is wrong.
    pub fn restore(&self, id: WorkspaceId) -> Result<Workspace, AppError> {
        let workspace = self.require_workspace(id)?;
        if workspace.status != WorkspaceStatus::Archived {
            return Err(AppError::user(format!(
                "workspace {:?} is not archived (status is {:?})",
                id, workspace.status
            )));
        }
        self.repository.update(Workspace {
            status: WorkspaceStatus::Active,
            ..workspace
        })
    }

    /// Unlink (delete) a workspace (§6.1: "Deleting a workspace link
    /// removes the workspace's row and watcher registration; it does NOT
    /// delete AI Cache or Student Memory by default"). This method only
    /// ever touches the `workspaces` row -- AI Cache/Student Memory
    /// deletion is Memory/Indexing Module responsibility (§46.2: single
    /// owner per table) and is explicitly out of scope here.
    pub fn unlink(&self, id: WorkspaceId) -> Result<(), AppError> {
        let workspace = self.require_workspace(id)?;
        self.repository.delete(id)?;

        self.events.publish(AppEvent {
            id: None,
            event_type: EventType::WorkspaceRemoved,
            payload: serde_json::json!({
                "workspace_id": workspace.id.0,
                "root_path": workspace.root_path,
            }),
            occurred_at: now_iso8601(),
        })?;
        Ok(())
    }

    fn require_workspace(&self, id: WorkspaceId) -> Result<Workspace, AppError> {
        self.repository
            .find_by_id(id)?
            .ok_or_else(|| AppError::user(format!("workspace {id:?} not found")))
    }
}

/// §45.1 "Workspace Errors: root folder missing/unreadable" -- validated at
/// link time so a bad path never enters the repository with an `Active`
/// status it hasn't earned. A missing/unreadable root at *watch* time
/// (e.g. a later-disconnected drive) is instead the "unavailable"
/// sub-state described in §45.1, handled by the Watcher Module, not here.
fn validate_root_path(root_path: &str) -> Result<(), AppError> {
    let path = std::path::Path::new(root_path);
    if !path.is_dir() {
        return Err(AppError::workspace(format!(
            "'{root_path}' is not a readable directory"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_events::InMemoryEventBus;

    use crate::testing::InMemoryWorkspaceRepository;

    fn engine() -> (WorkspaceEngine, Arc<InMemoryEventBus>) {
        let repository: Arc<dyn WorkspaceRepository> = Arc::new(InMemoryWorkspaceRepository::new());
        let events = Arc::new(InMemoryEventBus::new());
        let events_dyn: Arc<dyn EventBus> = events.clone();
        (WorkspaceEngine::new(repository, events_dyn), events)
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-workspace-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn engine_exposes_the_injected_dependencies() {
        let (engine, events) = engine();
        assert!(engine.repository().list().unwrap().is_empty());
        let events_dyn: Arc<dyn EventBus> = events.clone();
        assert!(Arc::ptr_eq(engine.events(), &events_dyn));
    }

    #[test]
    fn link_creates_an_active_workspace_and_publishes_workspace_added() {
        let (engine, events) = engine();
        let root = temp_dir("link");

        let workspace = engine
            .link(root.to_str().unwrap(), "My Workspace")
            .unwrap();

        assert_eq!(workspace.status, WorkspaceStatus::Active);
        assert_eq!(workspace.display_name, "My Workspace");

        let published = events.published_events();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].event_type, EventType::WorkspaceAdded);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn link_rejects_a_missing_root_path() {
        let (engine, _events) = engine();
        let missing = std::env::temp_dir().join("atlas-workspace-missing-xyz");
        let err = engine.link(missing.to_str().unwrap(), "X").unwrap_err();
        assert_eq!(err.category, atlas_utils::error::ErrorCategory::WorkspaceError);
    }

    #[test]
    fn link_rejects_empty_display_name() {
        let (engine, _events) = engine();
        let root = temp_dir("empty-name");
        assert!(engine.link(root.to_str().unwrap(), "").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_updates_display_name_without_touching_root_path() {
        let (engine, _events) = engine();
        let root = temp_dir("rename");
        let workspace = engine.link(root.to_str().unwrap(), "Old Name").unwrap();

        let renamed = engine.rename(workspace.id, "New Name").unwrap();
        assert_eq!(renamed.display_name, "New Name");
        assert_eq!(renamed.root_path, workspace.root_path);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_missing_workspace_is_an_error() {
        let (engine, _events) = engine();
        assert!(engine.rename(WorkspaceId(999), "X").is_err());
    }

    #[test]
    fn archive_then_restore_round_trips_status() {
        let (engine, _events) = engine();
        let root = temp_dir("archive-restore");
        let workspace = engine.link(root.to_str().unwrap(), "Archivable").unwrap();

        let archived = engine.archive(workspace.id).unwrap();
        assert_eq!(archived.status, WorkspaceStatus::Archived);

        let restored = engine.restore(workspace.id).unwrap();
        assert_eq!(restored.status, WorkspaceStatus::Active);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_is_idempotent() {
        let (engine, _events) = engine();
        let root = temp_dir("archive-idempotent");
        let workspace = engine.link(root.to_str().unwrap(), "W").unwrap();

        engine.archive(workspace.id).unwrap();
        let archived_again = engine.archive(workspace.id).unwrap();
        assert_eq!(archived_again.status, WorkspaceStatus::Archived);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_a_non_archived_workspace_is_an_error() {
        let (engine, _events) = engine();
        let root = temp_dir("restore-active");
        let workspace = engine.link(root.to_str().unwrap(), "W").unwrap();

        assert!(engine.restore(workspace.id).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unlink_removes_the_workspace_and_publishes_workspace_removed() {
        let (engine, events) = engine();
        let root = temp_dir("unlink");
        let workspace = engine.link(root.to_str().unwrap(), "W").unwrap();

        engine.unlink(workspace.id).unwrap();
        assert!(engine.get(workspace.id).unwrap().is_none());

        let published = events.published_events();
        assert_eq!(published.len(), 2);
        assert_eq!(published[1].event_type, EventType::WorkspaceRemoved);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unlink_missing_workspace_is_an_error() {
        let (engine, _events) = engine();
        assert!(engine.unlink(WorkspaceId(999)).is_err());
    }

    #[test]
    fn list_returns_every_linked_workspace() {
        let (engine, _events) = engine();
        let root1 = temp_dir("list-1");
        let root2 = temp_dir("list-2");
        engine.link(root1.to_str().unwrap(), "One").unwrap();
        engine.link(root2.to_str().unwrap(), "Two").unwrap();

        assert_eq!(engine.list().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&root1);
        let _ = std::fs::remove_dir_all(&root2);
    }
}
