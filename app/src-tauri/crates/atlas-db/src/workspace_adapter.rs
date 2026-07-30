//! SQLite-backed `WorkspaceRepository` (§33.1). Implements the interface
//! owned by atlas-workspace; the domain crate never depends on this crate.

use atlas_types::ids::WorkspaceId;
use atlas_types::workspace::{Workspace, WorkspaceStatus};
use atlas_utils::AppError;
use atlas_workspace::WorkspaceRepository;
use rusqlite::{params, OptionalExtension, Row};

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

fn status_to_str(status: &WorkspaceStatus) -> &'static str {
    match status {
        WorkspaceStatus::Unlinked => "unlinked",
        WorkspaceStatus::Linking => "linking",
        WorkspaceStatus::Indexing => "indexing",
        WorkspaceStatus::Active => "active",
        WorkspaceStatus::Archived => "archived",
    }
}

fn status_from_str(value: &str) -> Result<WorkspaceStatus, AppError> {
    match value {
        "unlinked" => Ok(WorkspaceStatus::Unlinked),
        "linking" => Ok(WorkspaceStatus::Linking),
        "indexing" => Ok(WorkspaceStatus::Indexing),
        "active" => Ok(WorkspaceStatus::Active),
        "archived" => Ok(WorkspaceStatus::Archived),
        other => Err(AppError::storage(format!(
            "unrecognized workspace status in database: {other}"
        ))),
    }
}

type WorkspaceRow = (i64, String, String, String, String, Option<String>);

fn row_to_workspace(row: &Row<'_>) -> rusqlite::Result<WorkspaceRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn tuple_to_workspace(tuple: WorkspaceRow) -> Result<Workspace, AppError> {
    let (id, root_path, display_name, status, created_at, last_indexed_at) = tuple;
    Ok(Workspace {
        id: WorkspaceId(id),
        root_path,
        display_name,
        status: status_from_str(&status)?,
        created_at,
        last_indexed_at,
    })
}

const SELECT_COLUMNS: &str =
    "id, root_path, display_name, status, created_at, last_indexed_at FROM workspaces";

impl WorkspaceRepository for SqliteWorkspaceRepository {
    fn find_by_id(&self, id: WorkspaceId) -> Result<Option<Workspace>, AppError> {
        let conn = self.connection.lock()?;
        let result = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} WHERE id = ?1"),
                params![id.0],
                row_to_workspace,
            )
            .optional()
            .map_err(|e| AppError::storage(format!("workspace find_by_id failed: {e}")))?;
        result.map(tuple_to_workspace).transpose()
    }

    fn list(&self) -> Result<Vec<Workspace>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(&format!("SELECT {SELECT_COLUMNS} ORDER BY id ASC"))
            .map_err(|e| AppError::storage(format!("workspace list prepare failed: {e}")))?;
        let rows = stmt
            .query_map([], row_to_workspace)
            .map_err(|e| AppError::storage(format!("workspace list query failed: {e}")))?;

        let mut workspaces = Vec::new();
        for row in rows {
            let tuple =
                row.map_err(|e| AppError::storage(format!("workspace row read failed: {e}")))?;
            workspaces.push(tuple_to_workspace(tuple)?);
        }
        Ok(workspaces)
    }

    fn insert(&self, workspace: Workspace) -> Result<Workspace, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO workspaces (root_path, display_name, status, created_at, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                workspace.root_path,
                workspace.display_name,
                status_to_str(&workspace.status),
                workspace.created_at,
                workspace.last_indexed_at,
            ],
        )
        .map_err(|e| {
            if e.to_string().to_lowercase().contains("unique") {
                AppError::user(format!(
                    "a workspace is already linked to '{}'",
                    workspace.root_path
                ))
            } else {
                AppError::storage(format!("workspace insert failed: {e}"))
            }
        })?;
        let id = conn.last_insert_rowid();
        Ok(Workspace {
            id: WorkspaceId(id),
            ..workspace
        })
    }

    fn update(&self, workspace: Workspace) -> Result<Workspace, AppError> {
        let conn = self.connection.lock()?;
        let affected = conn
            .execute(
                "UPDATE workspaces
                 SET root_path = ?1, display_name = ?2, status = ?3, created_at = ?4, last_indexed_at = ?5
                 WHERE id = ?6",
                params![
                    workspace.root_path,
                    workspace.display_name,
                    status_to_str(&workspace.status),
                    workspace.created_at,
                    workspace.last_indexed_at,
                    workspace.id.0,
                ],
            )
            .map_err(|e| AppError::storage(format!("workspace update failed: {e}")))?;

        if affected == 0 {
            return Err(AppError::user(format!(
                "workspace {:?} not found",
                workspace.id
            )));
        }
        Ok(workspace)
    }

    fn delete(&self, id: WorkspaceId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id.0])
            .map_err(|e| AppError::storage(format!("workspace delete failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(root_path: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(0),
            root_path: root_path.to_string(),
            display_name: "Sample".to_string(),
            status: WorkspaceStatus::Active,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            last_indexed_at: None,
        }
    }

    fn repo() -> SqliteWorkspaceRepository {
        SqliteWorkspaceRepository::new(SqliteConnection::open(":memory:"))
    }

    #[test]
    fn insert_assigns_an_id_and_persists_the_row() {
        let repo = repo();
        let inserted = repo.insert(sample("/tmp/ws-a")).unwrap();
        assert_ne!(inserted.id.0, 0);
        assert!(repo.find_by_id(inserted.id).unwrap().is_some());
    }

    #[test]
    fn insert_duplicate_root_path_is_a_user_error() {
        let repo = repo();
        repo.insert(sample("/tmp/ws-dup")).unwrap();
        let err = repo.insert(sample("/tmp/ws-dup")).unwrap_err();
        assert_eq!(err.category, atlas_utils::error::ErrorCategory::UserError);
    }

    #[test]
    fn list_returns_all_workspaces_in_insertion_order() {
        let repo = repo();
        repo.insert(sample("/tmp/ws-1")).unwrap();
        repo.insert(sample("/tmp/ws-2")).unwrap();
        let all = repo.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].root_path, "/tmp/ws-1");
        assert_eq!(all[1].root_path, "/tmp/ws-2");
    }

    #[test]
    fn update_persists_changed_fields() {
        let repo = repo();
        let inserted = repo.insert(sample("/tmp/ws-update")).unwrap();
        let mut updated = inserted.clone();
        updated.display_name = "Renamed".to_string();
        updated.status = WorkspaceStatus::Archived;
        repo.update(updated).unwrap();

        let fetched = repo.find_by_id(inserted.id).unwrap().unwrap();
        assert_eq!(fetched.display_name, "Renamed");
        assert_eq!(fetched.status, WorkspaceStatus::Archived);
    }

    #[test]
    fn update_missing_workspace_is_an_error() {
        let repo = repo();
        let err = repo.update(sample("/tmp/missing")).unwrap_err();
        assert_eq!(err.category, atlas_utils::error::ErrorCategory::UserError);
    }

    #[test]
    fn delete_removes_the_row() {
        let repo = repo();
        let inserted = repo.insert(sample("/tmp/ws-delete")).unwrap();
        repo.delete(inserted.id).unwrap();
        assert!(repo.find_by_id(inserted.id).unwrap().is_none());
    }

    #[test]
    fn find_by_id_missing_returns_none() {
        let repo = repo();
        assert!(repo.find_by_id(WorkspaceId(999)).unwrap().is_none());
    }
}
