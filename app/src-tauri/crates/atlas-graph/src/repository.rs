//! `GraphRepository` interface (§33.5, §33.6). Implemented by atlas-db.

use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;

pub trait GraphRepository: Send + Sync {
    fn list_nodes_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConceptNode>, AppError>;
    fn insert_node(&self, node: ConceptNode) -> Result<ConceptNode, AppError>;
    fn find_node(&self, id: ConceptNodeId) -> Result<Option<ConceptNode>, AppError>;

    fn list_edges_for_node(&self, node_id: ConceptNodeId) -> Result<Vec<ConceptEdge>, AppError>;
    fn insert_edge(&self, edge: ConceptEdge) -> Result<ConceptEdge, AppError>;
    fn delete_edge(&self, id: ConceptEdgeId) -> Result<(), AppError>;
}
