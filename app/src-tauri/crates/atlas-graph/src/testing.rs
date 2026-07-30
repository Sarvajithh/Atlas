//! Testing infrastructure for this crate (§30): a dependency-free,
//! in-memory [`GraphRepository`] implementation for unit tests that don't
//! need `atlas-db`/SQLite.

use std::sync::Mutex;

use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;

use crate::repository::GraphRepository;

pub struct InMemoryGraphRepository {
    nodes: Mutex<Vec<ConceptNode>>,
    edges: Mutex<Vec<ConceptEdge>>,
}

impl InMemoryGraphRepository {
    pub fn new() -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            edges: Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemoryGraphRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphRepository for InMemoryGraphRepository {
    fn list_nodes_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConceptNode>, AppError> {
        let nodes = self
            .nodes
            .lock()
            .map_err(|_| AppError::user("graph node lock poisoned"))?;
        Ok(nodes
            .iter()
            .filter(|n| n.workspace_id == workspace_id)
            .cloned()
            .collect())
    }

    fn insert_node(&self, node: ConceptNode) -> Result<ConceptNode, AppError> {
        let mut nodes = self
            .nodes
            .lock()
            .map_err(|_| AppError::user("graph node lock poisoned"))?;
        nodes.push(node.clone());
        Ok(node)
    }

    fn find_node(&self, id: ConceptNodeId) -> Result<Option<ConceptNode>, AppError> {
        let nodes = self
            .nodes
            .lock()
            .map_err(|_| AppError::user("graph node lock poisoned"))?;
        Ok(nodes.iter().find(|n| n.id == id).cloned())
    }

    fn list_edges_for_node(&self, node_id: ConceptNodeId) -> Result<Vec<ConceptEdge>, AppError> {
        let edges = self
            .edges
            .lock()
            .map_err(|_| AppError::user("graph edge lock poisoned"))?;
        Ok(edges
            .iter()
            .filter(|e| e.from_node_id == node_id || e.to_node_id == node_id)
            .cloned()
            .collect())
    }

    fn insert_edge(&self, edge: ConceptEdge) -> Result<ConceptEdge, AppError> {
        let mut edges = self
            .edges
            .lock()
            .map_err(|_| AppError::user("graph edge lock poisoned"))?;
        edges.push(edge.clone());
        Ok(edge)
    }

    fn delete_edge(&self, id: ConceptEdgeId) -> Result<(), AppError> {
        let mut edges = self
            .edges
            .lock()
            .map_err(|_| AppError::user("graph edge lock poisoned"))?;
        edges.retain(|e| e.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::concept::RelationType;

    fn node(id: i64, workspace_id: i64) -> ConceptNode {
        ConceptNode {
            id: ConceptNodeId(id),
            workspace_id: WorkspaceId(workspace_id),
            label: "Derivatives".to_string(),
            description: None,
            created_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    fn edge(id: i64, from: i64, to: i64) -> ConceptEdge {
        ConceptEdge {
            id: ConceptEdgeId(id),
            from_node_id: ConceptNodeId(from),
            to_node_id: ConceptNodeId(to),
            relation_type: RelationType::PrerequisiteOf,
            weight: 1.0,
        }
    }

    #[test]
    fn list_nodes_for_workspace_filters_by_workspace() {
        let repo = InMemoryGraphRepository::new();
        repo.insert_node(node(1, 10)).unwrap();
        repo.insert_node(node(2, 20)).unwrap();
        assert_eq!(
            repo.list_nodes_for_workspace(WorkspaceId(10))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn find_node_returns_none_when_missing() {
        let repo = InMemoryGraphRepository::new();
        assert!(repo.find_node(ConceptNodeId(99)).unwrap().is_none());
    }

    #[test]
    fn list_edges_for_node_matches_either_endpoint() {
        let repo = InMemoryGraphRepository::new();
        repo.insert_edge(edge(1, 1, 2)).unwrap();
        repo.insert_edge(edge(2, 2, 3)).unwrap();

        assert_eq!(repo.list_edges_for_node(ConceptNodeId(2)).unwrap().len(), 2);
        assert_eq!(repo.list_edges_for_node(ConceptNodeId(1)).unwrap().len(), 1);
    }

    #[test]
    fn delete_edge_removes_only_the_matching_edge() {
        let repo = InMemoryGraphRepository::new();
        repo.insert_edge(edge(1, 1, 2)).unwrap();
        repo.insert_edge(edge(2, 2, 3)).unwrap();
        repo.delete_edge(ConceptEdgeId(1)).unwrap();

        assert_eq!(repo.list_edges_for_node(ConceptNodeId(2)).unwrap().len(), 1);
    }
}
