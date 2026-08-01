//! SQLite-backed `SettingsProvider` (§23, §33.12). This is the only crate
//! reading/writing the `settings` table directly; every other crate goes
//! through the `SettingsProvider` interface (§33.12).

use atlas_config::SettingsProvider;
use atlas_types::ids::WorkspaceId;
use atlas_types::settings::{SettingEntry, SettingsScope};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension};

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

fn scope_to_str(scope: &SettingsScope) -> &'static str {
    match scope {
        SettingsScope::Global => "global",
        SettingsScope::Workspace => "workspace",
    }
}

fn scope_from_str(value: &str) -> Result<SettingsScope, AppError> {
    match value {
        "global" => Ok(SettingsScope::Global),
        "workspace" => Ok(SettingsScope::Workspace),
        other => Err(AppError::storage(format!("unrecognized settings scope in database: {other}"))),
    }
}

impl SettingsProvider for SqliteSettingsProvider {
    fn get_global(&self, key: &str) -> Result<Option<SettingEntry>, AppError> {
        let conn = self.connection.lock()?;
        conn.query_row(
            "SELECT key, value, value_type, scope, workspace_id, updated_at FROM settings WHERE key = ?1 AND scope = 'global'",
            params![key],
            |row| {
                let scope: String = row.get(3)?;
                let workspace_id: Option<i64> = row.get(4)?;
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, scope, workspace_id, row.get::<_, String>(5)?))
            },
        )
        .optional()
        .map_err(|e| AppError::storage(format!("settings get_global failed: {e}")))?
        .map(|(key, value, value_type, scope, workspace_id, updated_at)| {
            Ok(SettingEntry {
                key,
                value,
                value_type,
                scope: scope_from_str(&scope)?,
                workspace_id: workspace_id.map(WorkspaceId),
                updated_at,
            })
        })
        .transpose()
    }

    fn get_for_workspace(&self, key: &str, workspace_id: WorkspaceId) -> Result<Option<SettingEntry>, AppError> {
        let result = {
            let conn = self.connection.lock()?;
            conn.query_row(
                "SELECT key, value, value_type, scope, workspace_id, updated_at FROM settings WHERE key = ?1 AND scope = 'workspace' AND workspace_id = ?2",
                params![key, workspace_id.0],
                |row| {
                    let scope: String = row.get(3)?;
                    let ws: Option<i64> = row.get(4)?;
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, scope, ws, row.get::<_, String>(5)?))
                },
            )
            .optional()
            .map_err(|e| AppError::storage(format!("settings get_for_workspace failed: {e}")))?
            // `conn` (the lock guard) is dropped at the end of this block,
            // *before* the fallback call below re-locks the same mutex via
            // `get_global` -- without this scope, the fallback path would
            // deadlock on its own lock.
        };

        match result {
            Some((key, value, value_type, scope, workspace_id, updated_at)) => Ok(Some(SettingEntry {
                key,
                value,
                value_type,
                scope: scope_from_str(&scope)?,
                workspace_id: workspace_id.map(WorkspaceId),
                updated_at,
            })),
            // Fall back to the global value for this key when no
            // workspace-scoped override exists (§23's layered override
            // model: workspace overrides global, not "workspace replaces
            // global entirely").
            None => self.get_global(key),
        }
    }

    fn set(&self, entry: SettingEntry) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        let scope = scope_to_str(&entry.scope);
        let workspace_id = entry.workspace_id.map(|w| w.0);

        // No reliance on an ON CONFLICT clause here: SQLite's UNIQUE index
        // treats NULL `workspace_id` values as all-distinct, which would
        // let global entries (workspace_id = NULL) silently duplicate
        // instead of upserting. Check-then-update-or-insert instead.
        let affected = conn
            .execute(
                "UPDATE settings SET value = ?1, value_type = ?2, updated_at = ?3
                 WHERE key = ?4 AND scope = ?5 AND (workspace_id = ?6 OR (workspace_id IS NULL AND ?6 IS NULL))",
                params![entry.value, entry.value_type, entry.updated_at, entry.key, scope, workspace_id],
            )
            .map_err(|e| AppError::storage(format!("settings update failed: {e}")))?;

        if affected == 0 {
            conn.execute(
                "INSERT INTO settings (key, value, value_type, scope, workspace_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![entry.key, entry.value, entry.value_type, scope, workspace_id, entry.updated_at],
            )
            .map_err(|e| AppError::storage(format!("settings insert failed: {e}")))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> SqliteSettingsProvider {
        SqliteSettingsProvider::new(SqliteConnection::open(":memory:"))
    }

    fn entry(key: &str, value: &str) -> SettingEntry {
        SettingEntry {
            key: key.to_string(),
            value: value.to_string(),
            value_type: "string".to_string(),
            scope: SettingsScope::Global,
            workspace_id: None,
            updated_at: "t1".to_string(),
        }
    }

    #[test]
    fn get_global_returns_none_for_an_unset_key() {
        assert!(provider().get_global("ollama.host").unwrap().is_none());
    }

    #[test]
    fn set_then_get_global_round_trips() {
        let provider = provider();
        provider.set(entry("ollama.host", "localhost")).unwrap();
        assert_eq!(provider.get_global("ollama.host").unwrap().unwrap().value, "localhost");
    }

    #[test]
    fn set_twice_updates_in_place_rather_than_duplicating() {
        let provider = provider();
        provider.set(entry("ollama.host", "localhost")).unwrap();
        provider.set(entry("ollama.host", "192.168.1.10")).unwrap();
        assert_eq!(provider.get_global("ollama.host").unwrap().unwrap().value, "192.168.1.10");
    }

    #[test]
    fn workspace_scoped_value_overrides_global_for_that_workspace() {
        let provider = provider();
        provider.set(entry("ollama.host", "localhost")).unwrap();
        provider
            .set(SettingEntry {
                scope: SettingsScope::Workspace,
                workspace_id: Some(WorkspaceId(1)),
                value: "workspace-override".to_string(),
                ..entry("ollama.host", "unused")
            })
            .unwrap();

        assert_eq!(
            provider.get_for_workspace("ollama.host", WorkspaceId(1)).unwrap().unwrap().value,
            "workspace-override"
        );
        // A different, unconfigured workspace still sees the global value.
        assert_eq!(
            provider.get_for_workspace("ollama.host", WorkspaceId(2)).unwrap().unwrap().value,
            "localhost"
        );
    }
}
