//! Testing infrastructure for this crate (§30): a dependency-free,
//! in-memory [`GraphRepository`] implementation for unit tests that don't
//! need `atlas-db`/SQLite.

use std::sync::Mutex;

use atlas_types::concept::{ConceptEdge, ConceptNode, RelationType};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, DocumentId, WorkspaceId};
use atlas_utils::AppError;

use crate::repository::GraphRepository;

pub struct InMemoryGraphRepository {
    nodes: Mutex<Vec<ConceptNode>>,
    edges: Mutex<Vec<ConceptEdge>>,
    next_node_id: std::sync::atomic::AtomicI64,
    next_edge_id: std::sync::atomic::AtomicI64,
    /// (node_id, document_id) provenance pairs recorded via
    /// `record_node_source`.
    sources: Mutex<Vec<(ConceptNodeId, DocumentId)>>,
}

impl InMemoryGraphRepository {
    pub fn new() -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            edges: Mutex::new(Vec::new()),
            next_node_id: std::sync::atomic::AtomicI64::new(1),
            next_edge_id: std::sync::atomic::AtomicI64::new(1),
            sources: Mutex::new(Vec::new()),
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
        // Mirrors `SqliteGraphRepository`'s auto-increment rowid behavior:
        // an id of 0 (the "not yet persisted" sentinel every caller in this
        // codebase uses) is assigned a fresh id here; a caller-supplied
        // non-zero id (existing tests construct nodes with explicit ids
        // directly) is respected as-is.
        let node = if node.id.0 == 0 {
            let id = self.next_node_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            ConceptNode { id: ConceptNodeId(id), ..node }
        } else {
            node
        };
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

    fn find_node_by_label(
        &self,
        workspace_id: WorkspaceId,
        label: &str,
    ) -> Result<Option<ConceptNode>, AppError> {
        let nodes = self
            .nodes
            .lock()
            .map_err(|_| AppError::user("graph node lock poisoned"))?;
        Ok(nodes
            .iter()
            .find(|n| n.workspace_id == workspace_id && n.label.eq_ignore_ascii_case(label))
            .cloned())
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
        let edge = if edge.id.0 == 0 {
            let id = self.next_edge_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            ConceptEdge { id: ConceptEdgeId(id), ..edge }
        } else {
            edge
        };
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

    fn find_edge(
        &self,
        from_node_id: ConceptNodeId,
        to_node_id: ConceptNodeId,
        relation_type: &RelationType,
    ) -> Result<Option<ConceptEdge>, AppError> {
        let edges = self
            .edges
            .lock()
            .map_err(|_| AppError::user("graph edge lock poisoned"))?;
        Ok(edges
            .iter()
            .find(|e| {
                e.from_node_id == from_node_id
                    && e.to_node_id == to_node_id
                    && &e.relation_type == relation_type
            })
            .cloned())
    }

    fn record_node_source(&self, node_id: ConceptNodeId, document_id: DocumentId) -> Result<(), AppError> {
        let mut sources = self
            .sources
            .lock()
            .map_err(|_| AppError::user("graph source lock poisoned"))?;
        if !sources.iter().any(|(n, d)| *n == node_id && *d == document_id) {
            sources.push((node_id, document_id));
        }
        Ok(())
    }

    fn list_source_documents(&self, node_id: ConceptNodeId) -> Result<Vec<DocumentId>, AppError> {
        let sources = self
            .sources
            .lock()
            .map_err(|_| AppError::user("graph source lock poisoned"))?;
        Ok(sources.iter().filter(|(n, _)| *n == node_id).map(|(_, d)| *d).collect())
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
