//! `WorkspaceRepository` interface (§33.1). Implemented by atlas-db's SQLite
//! adapter; consumed here through Dependency Inversion.

use atlas_types::ids::WorkspaceId;
use atlas_types::workspace::Workspace;
use atlas_utils::AppError;

pub trait WorkspaceRepository: Send + Sync {
    fn find_by_id(&self, id: WorkspaceId) -> Result<Option<Workspace>, AppError>;
    fn list(&self) -> Result<Vec<Workspace>, AppError>;
    fn insert(&self, workspace: Workspace) -> Result<Workspace, AppError>;
    fn update(&self, workspace: Workspace) -> Result<Workspace, AppError>;
    fn delete(&self, id: WorkspaceId) -> Result<(), AppError>;
}
