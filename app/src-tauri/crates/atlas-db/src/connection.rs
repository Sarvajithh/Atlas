//! Shared SQLite connection handle. All adapters in this crate operate over
//! one `SqliteConnection`, wired in by atlas-core at startup (§41 step 3:
//! "Open Database").

/// Placeholder connection handle. The concrete `rusqlite`/`sqlx` dependency
/// and migration runner are introduced in the database-implementation
/// milestone (see crate-level Cargo.toml note).
#[derive(Clone)]
pub struct SqliteConnection {
    database_path: String,
}

impl SqliteConnection {
    pub fn open(database_path: impl Into<String>) -> Self {
        Self {
            database_path: database_path.into(),
        }
    }

    pub fn database_path(&self) -> &str {
        &self.database_path
    }
}
