//! SQLite-backed `ModelRegistryRepository` (§37, §33.13). No crate outside
//! atlas-models queries `model_registry` directly (§37.3); this adapter is
//! consumed only through atlas-models' interface.

use atlas_models::ModelRegistryRepository;
use atlas_types::ids::ModelRegistryId;
use atlas_types::model::{EngineRole, ModelRegistryEntry, ModelStatus};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

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

fn role_to_str(role: EngineRole) -> &'static str {
    match role {
        EngineRole::Vision => "vision",
        EngineRole::Ocr => "ocr",
        EngineRole::Embedding => "embedding",
        EngineRole::Retriever => "retriever",
        EngineRole::Reranker => "reranker",
        EngineRole::Tutor => "tutor",
        EngineRole::Reasoning => "reasoning",
        EngineRole::Planner => "planner",
        EngineRole::Memory => "memory",
        EngineRole::Analytics => "analytics",
    }
}

fn role_from_str(value: &str) -> Result<EngineRole, AppError> {
    match value {
        "vision" => Ok(EngineRole::Vision),
        "ocr" => Ok(EngineRole::Ocr),
        "embedding" => Ok(EngineRole::Embedding),
        "retriever" => Ok(EngineRole::Retriever),
        "reranker" => Ok(EngineRole::Reranker),
        "tutor" => Ok(EngineRole::Tutor),
        "reasoning" => Ok(EngineRole::Reasoning),
        "planner" => Ok(EngineRole::Planner),
        "memory" => Ok(EngineRole::Memory),
        "analytics" => Ok(EngineRole::Analytics),
        other => Err(AppError::storage(format!("unrecognized engine_role in database: {other}"))),
    }
}

fn status_to_str(status: &ModelStatus) -> &'static str {
    match status {
        ModelStatus::Available => "available",
        ModelStatus::Loading => "loading",
        ModelStatus::Unavailable => "unavailable",
        ModelStatus::Error => "error",
    }
}

fn status_from_str(value: &str) -> Result<ModelStatus, AppError> {
    match value {
        "available" => Ok(ModelStatus::Available),
        "loading" => Ok(ModelStatus::Loading),
        "unavailable" => Ok(ModelStatus::Unavailable),
        "error" => Ok(ModelStatus::Error),
        other => Err(AppError::storage(format!("unrecognized model status in database: {other}"))),
    }
}

#[allow(clippy::type_complexity)]
type RegistryRow = (i64, String, String, String, u32, Option<u64>, String, String, String, bool);

fn row_to_tuple(row: &Row<'_>) -> rusqlite::Result<RegistryRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn tuple_to_entry(tuple: RegistryRow) -> Result<ModelRegistryEntry, AppError> {
    let (id, model_identifier, engine_role, capabilities, context_length, vram_requirement, status, version, supported_tasks, is_selected_for_role) =
        tuple;
    Ok(ModelRegistryEntry {
        id: ModelRegistryId(id),
        model_identifier,
        engine_role: role_from_str(&engine_role)?,
        capabilities: serde_json::from_str(&capabilities)
            .map_err(|e| AppError::storage(format!("invalid capabilities JSON: {e}")))?,
        context_length,
        vram_requirement,
        status: status_from_str(&status)?,
        version,
        supported_tasks: serde_json::from_str(&supported_tasks)
            .map_err(|e| AppError::storage(format!("invalid supported_tasks JSON: {e}")))?,
        is_selected_for_role,
    })
}

const SELECT_COLUMNS: &str = "id, model_identifier, engine_role, capabilities, context_length, vram_requirement, status, version, supported_tasks, is_selected_for_role FROM model_registry";

impl ModelRegistryRepository for SqliteModelRegistryRepository {
    fn list(&self) -> Result<Vec<ModelRegistryEntry>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!("SELECT {SELECT_COLUMNS} ORDER BY id ASC"))
            .map_err(|e| AppError::storage(format!("model_registry list prepare failed: {e}")))?;
        let rows = stmt
            .query_map([], row_to_tuple)
            .map_err(|e| AppError::storage(format!("model_registry list query failed: {e}")))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(tuple_to_entry(
                row.map_err(|e| AppError::storage(format!("model_registry row read failed: {e}")))?,
            )?);
        }
        Ok(entries)
    }

    fn find_for_role(&self, role: EngineRole) -> Result<Option<ModelRegistryEntry>, AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        atlas_utils::log_info!("[ModelRegistry/SQLite] find_for_role querying role={}", role_to_str(role));
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} WHERE engine_role = ?1 AND is_selected_for_role = 1 LIMIT 1"),
                params![role_to_str(role)],
                row_to_tuple,
            )
            .optional()
            .map_err(|e| AppError::storage(format!("model_registry find_for_role failed: {e}")))?;
        atlas_utils::log_info!("[ModelRegistry/SQLite] find_for_role role={} found_row={}", role_to_str(role), result.is_some());
        result.map(tuple_to_entry).transpose()
    }

    fn upsert(&self, entry: ModelRegistryEntry) -> Result<ModelRegistryEntry, AppError> {
        let conn = self.connection.lock()?;
        let capabilities = entry.capabilities.to_string();
        let supported_tasks = entry.supported_tasks.to_string();

        if entry.id.0 == 0 {
            conn.execute(
                "INSERT INTO model_registry (model_identifier, engine_role, capabilities, context_length, vram_requirement, status, version, supported_tasks, is_selected_for_role)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.model_identifier,
                    role_to_str(entry.engine_role),
                    capabilities,
                    entry.context_length,
                    entry.vram_requirement,
                    status_to_str(&entry.status),
                    entry.version,
                    supported_tasks,
                    entry.is_selected_for_role,
                ],
            )
            .map_err(|e| AppError::storage(format!("model_registry insert failed: {e}")))?;
            let id = conn.last_insert_rowid();
            return Ok(ModelRegistryEntry { id: ModelRegistryId(id), ..entry });
        }

        let affected = conn
            .execute(
                "UPDATE model_registry SET model_identifier = ?1, engine_role = ?2, capabilities = ?3, context_length = ?4, vram_requirement = ?5, status = ?6, version = ?7, supported_tasks = ?8, is_selected_for_role = ?9
                 WHERE id = ?10",
                params![
                    entry.model_identifier,
                    role_to_str(entry.engine_role),
                    capabilities,
                    entry.context_length,
                    entry.vram_requirement,
                    status_to_str(&entry.status),
                    entry.version,
                    supported_tasks,
                    entry.is_selected_for_role,
                    entry.id.0,
                ],
            )
            .map_err(|e| AppError::storage(format!("model_registry update failed: {e}")))?;

        if affected == 0 {
            conn.execute(
                "INSERT INTO model_registry (id, model_identifier, engine_role, capabilities, context_length, vram_requirement, status, version, supported_tasks, is_selected_for_role)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    entry.id.0,
                    entry.model_identifier,
                    role_to_str(entry.engine_role),
                    capabilities,
                    entry.context_length,
                    entry.vram_requirement,
                    status_to_str(&entry.status),
                    entry.version,
                    supported_tasks,
                    entry.is_selected_for_role,
                ],
            )
            .map_err(|e| AppError::storage(format!("model_registry insert-with-id failed: {e}")))?;
        }

        Ok(entry)
    }
}

impl atlas_models::ModelProvider for SqliteModelRegistryRepository {
    fn current_model_for(&self, role: EngineRole) -> Result<ModelRegistryEntry, AppError> {
        ModelRegistryRepository::find_for_role(self, role)?
            .ok_or_else(|| AppError::model(format!("no model currently assigned to {role:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> SqliteModelRegistryRepository {
        SqliteModelRegistryRepository::new(SqliteConnection::open(":memory:"))
    }

    fn sample(role: EngineRole, selected: bool) -> ModelRegistryEntry {
        ModelRegistryEntry {
            id: ModelRegistryId(0),
            model_identifier: "llama3.1".to_string(),
            engine_role: role,
            capabilities: serde_json::json!(["text-generation"]),
            context_length: 8192,
            vram_requirement: Some(8_000_000_000),
            status: ModelStatus::Available,
            version: "1".to_string(),
            supported_tasks: serde_json::json!(["chat"]),
            is_selected_for_role: selected,
        }
    }

    #[test]
    fn upsert_assigns_id_and_persists() {
        let repo = repo();
        let entry = repo.upsert(sample(EngineRole::Tutor, true)).unwrap();
        assert_ne!(entry.id.0, 0);
        assert_eq!(repo.list().unwrap().len(), 1);
    }

    #[test]
    fn find_for_role_only_returns_selected_entry() {
        let repo = repo();
        repo.upsert(sample(EngineRole::Tutor, false)).unwrap();
        assert!(repo.find_for_role(EngineRole::Tutor).unwrap().is_none());

        repo.upsert(sample(EngineRole::Tutor, true)).unwrap();
        assert!(repo.find_for_role(EngineRole::Tutor).unwrap().is_some());
    }

    #[test]
    fn upsert_with_existing_id_updates_in_place() {
        let repo = repo();
        let inserted = repo.upsert(sample(EngineRole::Reasoning, true)).unwrap();

        let mut updated = inserted.clone();
        updated.model_identifier = "qwen2.5".to_string();
        repo.upsert(updated).unwrap();

        let all = repo.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].model_identifier, "qwen2.5");
    }

    #[test]
    fn capabilities_and_supported_tasks_round_trip_as_json() {
        let repo = repo();
        let inserted = repo.upsert(sample(EngineRole::Vision, true)).unwrap();
        let found = repo.find_for_role(EngineRole::Vision).unwrap().unwrap();
        assert_eq!(found.capabilities, inserted.capabilities);
        assert_eq!(found.supported_tasks, inserted.supported_tasks);
    }
}
