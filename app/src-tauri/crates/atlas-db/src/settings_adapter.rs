//! SQLite-backed `SettingsProvider` (§23, §33.12). This is the only crate
//! reading/writing the `settings` table directly; every other crate goes
//! through the `SettingsProvider` interface (§33.12).

use atlas_config::SettingsProvider;
use atlas_types::ids::WorkspaceId;
use atlas_types::settings::SettingEntry;
use atlas_utils::AppError;

use crate::connection::SqliteConnection;

pub struct SqliteSettingsProvider {
    connection: SqliteConnection,
}

impl SqliteSettingsProvider {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl SettingsProvider for SqliteSettingsProvider {
    fn get_global(&self, _key: &str) -> Result<Option<SettingEntry>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn get_for_workspace(
        &self,
        _key: &str,
        _workspace_id: WorkspaceId,
    ) -> Result<Option<SettingEntry>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn set(&self, _entry: SettingEntry) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
