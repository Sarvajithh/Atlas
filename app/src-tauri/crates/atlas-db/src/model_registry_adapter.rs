//! SQLite-backed `ModelRegistryRepository` (§37, §33.13). No crate outside
//! atlas-models queries `model_registry` directly (§37.3); this adapter is
//! consumed only through atlas-models' interface.

use atlas_models::ModelRegistryRepository;
use atlas_types::model::{EngineRole, ModelRegistryEntry};
use atlas_utils::AppError;

use crate::connection::SqliteConnection;

pub struct SqliteModelRegistryRepository {
    connection: SqliteConnection,
}

impl SqliteModelRegistryRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl ModelRegistryRepository for SqliteModelRegistryRepository {
    fn list(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn find_for_role(&self, _role: EngineRole) -> Result<Option<ModelRegistryEntry>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn upsert(&self, _entry: ModelRegistryEntry) -> Result<ModelRegistryEntry, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
