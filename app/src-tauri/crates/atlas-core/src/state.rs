//! Application State (§13).
//!
//! §13 draws a clear line: **SQLite is the source of truth for structured
//! state** (workspaces, documents, memory, settings); **in-memory backend
//! state is transient and reconstructable on restart**. This module is the
//! single owner of that transient, in-memory slice -- "no duplicated
//! state" means nothing here is a second copy of anything SQLite (via
//! `atlas-db`) already owns; it only tracks *which* persisted entity the
//! current process session is looking at right now.
//!
//! `AppState` is intentionally small and workspace-independent in shape
//! (it doesn't know what a workspace *means* -- only that one may be
//! "active" for the current session, per §9's navigation model).

use std::sync::RwLock;

use atlas_types::ids::{DocumentId, WorkspaceId};
use atlas_utils::AppError;

/// Transient, process-lifetime application state (§13). Never persisted;
/// on restart it starts empty and, per §41 step 4 ("Load Workspaces"), is
/// repopulated by reading from SQLite -- it is a cache of *session focus*,
/// not a cache of backend data (that's the frontend Zustand store's job,
/// §13, on the other side of the IPC boundary).
pub struct AppState {
    active_workspace_id: RwLock<Option<WorkspaceId>>,
    active_document_id: RwLock<Option<DocumentId>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_workspace_id: RwLock::new(None),
            active_document_id: RwLock::new(None),
        }
    }

    pub fn active_workspace_id(&self) -> Result<Option<WorkspaceId>, AppError> {
        Ok(*self
            .active_workspace_id
            .read()
            .map_err(|_| AppError::user("app state lock poisoned"))?)
    }

    pub fn set_active_workspace_id(&self, id: Option<WorkspaceId>) -> Result<(), AppError> {
        let mut guard = self
            .active_workspace_id
            .write()
            .map_err(|_| AppError::user("app state lock poisoned"))?;
        *guard = id;
        // Switching workspace clears the active document -- a document
        // from a different workspace can't stay "active" (§9: navigation
        // is workspace-scoped by default).
        let mut doc_guard = self
            .active_document_id
            .write()
            .map_err(|_| AppError::user("app state lock poisoned"))?;
        *doc_guard = None;
        Ok(())
    }

    pub fn active_document_id(&self) -> Result<Option<DocumentId>, AppError> {
        Ok(*self
            .active_document_id
            .read()
            .map_err(|_| AppError::user("app state lock poisoned"))?)
    }

    pub fn set_active_document_id(&self, id: Option<DocumentId>) -> Result<(), AppError> {
        let mut guard = self
            .active_document_id
            .write()
            .map_err(|_| AppError::user("app state lock poisoned"))?;
        *guard = id;
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_no_active_workspace_or_document() {
        let state = AppState::new();
        assert!(state.active_workspace_id().unwrap().is_none());
        assert!(state.active_document_id().unwrap().is_none());
    }

    #[test]
    fn set_active_workspace_id_is_reflected_in_getter() {
        let state = AppState::new();
        state.set_active_workspace_id(Some(WorkspaceId(1))).unwrap();
        assert_eq!(state.active_workspace_id().unwrap(), Some(WorkspaceId(1)));
    }

    #[test]
    fn switching_workspace_clears_active_document() {
        let state = AppState::new();
        state.set_active_workspace_id(Some(WorkspaceId(1))).unwrap();
        state.set_active_document_id(Some(DocumentId(5))).unwrap();
        assert_eq!(state.active_document_id().unwrap(), Some(DocumentId(5)));

        state.set_active_workspace_id(Some(WorkspaceId(2))).unwrap();
        assert!(state.active_document_id().unwrap().is_none());
    }

    #[test]
    fn clearing_active_workspace_also_clears_document() {
        let state = AppState::new();
        state.set_active_workspace_id(Some(WorkspaceId(1))).unwrap();
        state.set_active_document_id(Some(DocumentId(5))).unwrap();

        state.set_active_workspace_id(None).unwrap();
        assert!(state.active_workspace_id().unwrap().is_none());
        assert!(state.active_document_id().unwrap().is_none());
    }
}
