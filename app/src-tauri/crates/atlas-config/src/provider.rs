//! SettingsProvider interface (§23, §33.12).

use atlas_types::ids::WorkspaceId;
use atlas_types::settings::SettingEntry;
use atlas_utils::AppError;

/// The single entry point every crate uses to read/write configuration.
/// Concrete storage (the `settings` table, §33.12) is implemented by
/// atlas-db and injected at composition time (Dependency Inversion,
/// Governing Principle).
pub trait SettingsProvider: Send + Sync {
    fn get_global(&self, key: &str) -> Result<Option<SettingEntry>, AppError>;

    fn get_for_workspace(
        &self,
        key: &str,
        workspace_id: WorkspaceId,
    ) -> Result<Option<SettingEntry>, AppError>;

    fn set(&self, entry: SettingEntry) -> Result<(), AppError>;
}
