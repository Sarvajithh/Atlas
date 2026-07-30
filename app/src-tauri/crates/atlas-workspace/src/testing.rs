//! Testing infrastructure for this crate (§30): a dependency-free,
//! in-memory [`WorkspaceRepository`] implementation. This is deliberately
//! *just* storage -- insert/list/update/delete with no lifecycle rules
//! (§6.1's state-machine transitions are the Workspace Engine's
//! responsibility, which this task explicitly does not implement). It
//! exists so any crate that depends on `atlas-workspace` can write unit
//! tests without pulling in `atlas-db`/SQLite.

use std::sync::Mutex;

use atlas_types::ids::WorkspaceId;
use atlas_types::workspace::Workspace;
use atlas_utils::AppError;

use crate::repository::WorkspaceRepository;

pub struct InMemoryWorkspaceRepository {
    workspaces: Mutex<Vec<Workspace>>,
}

impl InMemoryWorkspaceRepository {
    pub fn new() -> Self {
        Self {
            workspaces: Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemoryWorkspaceRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceRepository for InMemoryWorkspaceRepository {
    fn find_by_id(&self, id: WorkspaceId) -> Result<Option<Workspace>, AppError> {
        let workspaces = self
            .workspaces
            .lock()
            .map_err(|_| AppError::user("workspace store lock poisoned"))?;
        Ok(workspaces.iter().find(|w| w.id == id).cloned())
    }

    fn list(&self) -> Result<Vec<Workspace>, AppError> {
        Ok(self
            .workspaces
            .lock()
            .map_err(|_| AppError::user("workspace store lock poisoned"))?
            .clone())
    }

    fn insert(&self, workspace: Workspace) -> Result<Workspace, AppError> {
        let mut workspaces = self
            .workspaces
            .lock()
            .map_err(|_| AppError::user("workspace store lock poisoned"))?;
        workspaces.push(workspace.clone());
        Ok(workspace)
    }

    fn update(&self, workspace: Workspace) -> Result<Workspace, AppError> {
        let mut workspaces = self
            .workspaces
            .lock()
            .map_err(|_| AppError::user("workspace store lock poisoned"))?;
        if let Some(existing) = workspaces.iter_mut().find(|w| w.id == workspace.id) {
            *existing = workspace.clone();
            Ok(workspace)
        } else {
            Err(AppError::user(format!(
                "workspace {:?} not found",
                workspace.id
            )))
        }
    }

    fn delete(&self, id: WorkspaceId) -> Result<(), AppError> {
        let mut workspaces = self
            .workspaces
            .lock()
            .map_err(|_| AppError::user("workspace store lock poisoned"))?;
        workspaces.retain(|w| w.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::workspace::WorkspaceStatus;

    fn sample(id: i64) -> Workspace {
        Workspace {
            id: WorkspaceId(id),
            root_path: "/tmp/workspace".to_string(),
            display_name: "Sample".to_string(),
            status: WorkspaceStatus::Active,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            last_indexed_at: None,
        }
    }

    #[test]
    fn insert_then_find_by_id_returns_the_workspace() {
        let repo = InMemoryWorkspaceRepository::new();
        repo.insert(sample(1)).unwrap();
        assert!(repo.find_by_id(WorkspaceId(1)).unwrap().is_some());
    }

    #[test]
    fn find_by_id_missing_returns_none() {
        let repo = InMemoryWorkspaceRepository::new();
        assert!(repo.find_by_id(WorkspaceId(99)).unwrap().is_none());
    }

    #[test]
    fn list_returns_all_inserted_workspaces() {
        let repo = InMemoryWorkspaceRepository::new();
        repo.insert(sample(1)).unwrap();
        repo.insert(sample(2)).unwrap();
        assert_eq!(repo.list().unwrap().len(), 2);
    }

    #[test]
    fn update_replaces_existing_workspace() {
        let repo = InMemoryWorkspaceRepository::new();
        repo.insert(sample(1)).unwrap();
        let mut updated = sample(1);
        updated.display_name = "Renamed".to_string();
        repo.update(updated).unwrap();
        assert_eq!(
            repo.find_by_id(WorkspaceId(1))
                .unwrap()
                .unwrap()
                .display_name,
            "Renamed"
        );
    }

    #[test]
    fn update_missing_workspace_is_an_error() {
        let repo = InMemoryWorkspaceRepository::new();
        assert!(repo.update(sample(1)).is_err());
    }

    #[test]
    fn delete_removes_the_workspace() {
        let repo = InMemoryWorkspaceRepository::new();
        repo.insert(sample(1)).unwrap();
        repo.delete(WorkspaceId(1)).unwrap();
        assert!(repo.find_by_id(WorkspaceId(1)).unwrap().is_none());
    }
}
