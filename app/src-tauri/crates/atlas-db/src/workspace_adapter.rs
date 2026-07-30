//! SQLite-backed `WorkspaceRepository` (§33.1). Implements the interface
//! owned by atlas-workspace; the domain crate never depends on this crate.

use atlas_types::ids::WorkspaceId;
use atlas_types::workspace::Workspace;
use atlas_utils::AppError;
use atlas_workspace::WorkspaceRepository;

use crate::connection::SqliteConnection;

pub struct SqliteWorkspaceRepository {
    connection: SqliteConnection,
}

impl SqliteWorkspaceRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl WorkspaceRepository for SqliteWorkspaceRepository {
    fn find_by_id(&self, _id: WorkspaceId) -> Result<Option<Workspace>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn list(&self) -> Result<Vec<Workspace>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert(&self, _workspace: Workspace) -> Result<Workspace, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn update(&self, _workspace: Workspace) -> Result<Workspace, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete(&self, _id: WorkspaceId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
