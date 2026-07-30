//! SQLite-backed `GraphRepository` (§33.5, §33.6). No dedicated graph
//! database -- concept nodes/edges are relational tables (§20).

use atlas_graph::GraphRepository;
use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;

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

impl GraphRepository for SqliteGraphRepository {
    fn list_nodes_for_workspace(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<Vec<ConceptNode>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert_node(&self, _node: ConceptNode) -> Result<ConceptNode, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn find_node(&self, _id: ConceptNodeId) -> Result<Option<ConceptNode>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn list_edges_for_node(&self, _node_id: ConceptNodeId) -> Result<Vec<ConceptEdge>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert_edge(&self, _edge: ConceptEdge) -> Result<ConceptEdge, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete_edge(&self, _id: ConceptEdgeId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
