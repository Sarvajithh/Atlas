//! `AnalyticsRepository` interface (§33.16). A materialized/cache table
//! conceptually belonging to AI Cache (§7.2) even though it is derived from
//! Student Memory data. Implemented by atlas-db.

use atlas_types::ids::WorkspaceId;
use atlas_types::memory::AnalyticsPoint;
use atlas_utils::AppError;

pub trait AnalyticsRepository: Send + Sync {
    fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<AnalyticsPoint>, AppError>;
    fn upsert(&self, point: AnalyticsPoint) -> Result<AnalyticsPoint, AppError>;
}
