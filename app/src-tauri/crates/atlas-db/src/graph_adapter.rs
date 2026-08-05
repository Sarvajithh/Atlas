//! SQLite-backed `GraphRepository` (§33.5, §33.6). No dedicated graph
//! database -- concept nodes/edges are relational tables (§20).

use atlas_graph::GraphRepository;
use atlas_types::concept::{ConceptEdge, ConceptNode, RelationType};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;
use rusqlite::{params, OptionalExtension, Row};

use crate::connection::SqliteConnection;

pub struct SqliteGraphRepository {
    connection: SqliteConnection,
}

impl SqliteGraphRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

fn relation_type_to_str(relation_type: &RelationType) -> &'static str {
    match relation_type {
        RelationType::PrerequisiteOf => "prerequisite-of",
        RelationType::RelatedTo => "related-to",
        RelationType::PartOf => "part-of",
    }
}

fn relation_type_from_str(value: &str) -> Result<RelationType, AppError> {
    match value {
        "prerequisite-of" => Ok(RelationType::PrerequisiteOf),
        "related-to" => Ok(RelationType::RelatedTo),
        "part-of" => Ok(RelationType::PartOf),
        other => Err(AppError::storage(format!(
            "unrecognized concept edge relation_type in database: {other}"
        ))),
    }
}

fn row_to_node(row: &Row<'_>) -> rusqlite::Result<ConceptNode> {
    Ok(ConceptNode {
        id: ConceptNodeId(row.get(0)?),
        workspace_id: WorkspaceId(row.get(1)?),
        label: row.get(2)?,
        description: row.get(3)?,
        created_at: row.get(4)?,
    })
}

impl GraphRepository for SqliteGraphRepository {
    fn list_nodes_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConceptNode>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, label, description, created_at \
                 FROM concept_nodes WHERE workspace_id = ?1 ORDER BY id ASC",
            )
            .map_err(|e| AppError::storage(format!("concept node list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![workspace_id.0], row_to_node)
            .map_err(|e| AppError::storage(format!("concept node list query failed: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::storage(format!("concept node row read failed: {e}")))
    }

    fn insert_node(&self, node: ConceptNode) -> Result<ConceptNode, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO concept_nodes (workspace_id, label, description, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![node.workspace_id.0, node.label, node.description, node.created_at],
        )
        .map_err(|e| AppError::storage(format!("concept node insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(ConceptNode { id: ConceptNodeId(id), ..node })
    }

    fn find_node(&self, id: ConceptNodeId) -> Result<Option<ConceptNode>, AppError> {
        let conn = self.connection.lock()?;
        conn.query_row(
            "SELECT id, workspace_id, label, description, created_at \
             FROM concept_nodes WHERE id = ?1",
            params![id.0],
            row_to_node,
        )
        .optional()
        .map_err(|e| AppError::storage(format!("concept node find failed: {e}")))
    }

    fn list_edges_for_node(&self, node_id: ConceptNodeId) -> Result<Vec<ConceptEdge>, AppError> {
        let conn = self.connection.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, from_node_id, to_node_id, relation_type, weight \
                 FROM concept_edges WHERE from_node_id = ?1 OR to_node_id = ?1 ORDER BY id ASC",
            )
            .map_err(|e| AppError::storage(format!("concept edge list prepare failed: {e}")))?;
        let rows = stmt
            .query_map(params![node_id.0], |row| {
                let relation_str: String = row.get(3)?;
                Ok((
                    ConceptEdgeId(row.get(0)?),
                    ConceptNodeId(row.get(1)?),
                    ConceptNodeId(row.get(2)?),
                    relation_str,
                    row.get::<_, f64>(4)? as f32,
                ))
            })
            .map_err(|e| AppError::storage(format!("concept edge list query failed: {e}")))?;

        let mut edges = Vec::new();
        for row in rows {
            let (id, from_node_id, to_node_id, relation_str, weight) =
                row.map_err(|e| AppError::storage(format!("concept edge row read failed: {e}")))?;
            let relation_type = relation_type_from_str(&relation_str)?;
            edges.push(ConceptEdge { id, from_node_id, to_node_id, relation_type, weight });
        }
        Ok(edges)
    }

    fn insert_edge(&self, edge: ConceptEdge) -> Result<ConceptEdge, AppError> {
        let conn = self.connection.lock()?;
        conn.execute(
            "INSERT INTO concept_edges (from_node_id, to_node_id, relation_type, weight) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                edge.from_node_id.0,
                edge.to_node_id.0,
                relation_type_to_str(&edge.relation_type),
                edge.weight,
            ],
        )
        .map_err(|e| AppError::storage(format!("concept edge insert failed: {e}")))?;
        let id = conn.last_insert_rowid();
        Ok(ConceptEdge { id: ConceptEdgeId(id), ..edge })
    }

    fn delete_edge(&self, id: ConceptEdgeId) -> Result<(), AppError> {
        let conn = self.connection.lock()?;
        conn.execute("DELETE FROM concept_edges WHERE id = ?1", params![id.0])
            .map_err(|e| AppError::storage(format!("concept edge delete failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> SqliteConnection {
        SqliteConnection::open(":memory:")
    }

    fn sample_node(workspace_id: i64, label: &str) -> ConceptNode {
        ConceptNode {
            id: ConceptNodeId(0),
            workspace_id: WorkspaceId(workspace_id),
            label: label.to_string(),
            description: Some("a concept".to_string()),
            created_at: "t1".to_string(),
        }
    }

    #[test]
    fn insert_then_find_node_round_trips() {
        let repo = SqliteGraphRepository::new(conn());
        let inserted = repo.insert_node(sample_node(1, "Gradient Descent")).unwrap();
        assert_ne!(inserted.id.0, 0);

        let found = repo.find_node(inserted.id).unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.label, "Gradient Descent");
        assert_eq!(found.workspace_id, WorkspaceId(1));
    }

    #[test]
    fn find_node_returns_none_not_error_when_missing() {
        let repo = SqliteGraphRepository::new(conn());
        let found = repo.find_node(ConceptNodeId(9999)).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn list_nodes_for_workspace_empty_case() {
        let repo = SqliteGraphRepository::new(conn());
        let nodes = repo.list_nodes_for_workspace(WorkspaceId(1)).unwrap();
        assert!(nodes.is_empty());
    }

    #[test]
    fn list_nodes_for_workspace_only_returns_matching_workspace() {
        let repo = SqliteGraphRepository::new(conn());
        repo.insert_node(sample_node(1, "Backprop")).unwrap();
        repo.insert_node(sample_node(1, "Chain Rule")).unwrap();
        repo.insert_node(sample_node(2, "Unrelated Workspace Concept")).unwrap();

        let nodes = repo.list_nodes_for_workspace(WorkspaceId(1)).unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.workspace_id == WorkspaceId(1)));
    }

    #[test]
    fn insert_and_list_edges_for_node() {
        let repo = SqliteGraphRepository::new(conn());
        let a = repo.insert_node(sample_node(1, "Derivatives")).unwrap();
        let b = repo.insert_node(sample_node(1, "Gradient Descent")).unwrap();

        let edge = repo
            .insert_edge(ConceptEdge {
                id: ConceptEdgeId(0),
                from_node_id: a.id,
                to_node_id: b.id,
                relation_type: RelationType::PrerequisiteOf,
                weight: 0.9,
            })
            .unwrap();
        assert_ne!(edge.id.0, 0);

        let edges_from_a = repo.list_edges_for_node(a.id).unwrap();
        assert_eq!(edges_from_a.len(), 1);
        assert_eq!(edges_from_a[0].relation_type, RelationType::PrerequisiteOf);

        let edges_from_b = repo.list_edges_for_node(b.id).unwrap();
        assert_eq!(edges_from_b.len(), 1);
    }

    #[test]
    fn delete_edge_removes_it() {
        let repo = SqliteGraphRepository::new(conn());
        let a = repo.insert_node(sample_node(1, "A")).unwrap();
        let b = repo.insert_node(sample_node(1, "B")).unwrap();
        let edge = repo
            .insert_edge(ConceptEdge {
                id: ConceptEdgeId(0),
                from_node_id: a.id,
                to_node_id: b.id,
                relation_type: RelationType::RelatedTo,
                weight: 0.5,
            })
            .unwrap();

        repo.delete_edge(edge.id).unwrap();

        assert!(repo.list_edges_for_node(a.id).unwrap().is_empty());
    }
}
