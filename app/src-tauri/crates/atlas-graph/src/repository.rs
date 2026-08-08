//! `GraphRepository` interface (§33.5, §33.6). Implemented by atlas-db.

use atlas_types::concept::{ConceptEdge, ConceptNode, RelationType};
use atlas_types::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};
use atlas_utils::AppError;

pub trait GraphRepository: Send + Sync {
    fn list_nodes_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<ConceptNode>, AppError>;
    fn insert_node(&self, node: ConceptNode) -> Result<ConceptNode, AppError>;
    fn find_node(&self, id: ConceptNodeId) -> Result<Option<ConceptNode>, AppError>;

    /// Case-insensitive exact-label lookup scoped to a workspace, used by
    /// the Concept Extraction pipeline (§20) to reuse an existing node
    /// rather than creating a duplicate every time the same concept is
    /// mentioned across documents/re-indexing passes.
    fn find_node_by_label(
        &self,
        workspace_id: WorkspaceId,
        label: &str,
    ) -> Result<Option<ConceptNode>, AppError>;

    fn list_edges_for_node(&self, node_id: ConceptNodeId) -> Result<Vec<ConceptEdge>, AppError>;
    fn insert_edge(&self, edge: ConceptEdge) -> Result<ConceptEdge, AppError>;
    fn delete_edge(&self, id: ConceptEdgeId) -> Result<(), AppError>;

    /// Exact (from, to, relation_type) lookup, used by the Concept
    /// Extraction pipeline to avoid inserting a duplicate edge when the
    /// same relation is re-derived (e.g. on re-indexing an unchanged
    /// section of a document that was previously extracted).
    fn find_edge(
        &self,
        from_node_id: ConceptNodeId,
        to_node_id: ConceptNodeId,
        relation_type: &RelationType,
    ) -> Result<Option<ConceptEdge>, AppError>;

    /// Records that `node_id` was (re)derived from `document_id` (Research
    /// Mode phase, §20). A join table, not a change to `ConceptNode`
    /// itself -- a node stays workspace-scoped (so the same concept
    /// mentioned in several documents dedupes to one node, as extraction
    /// already relies on), while this separately tracks *which* documents
    /// actually mention it, which is what distinguishes a within-one-
    /// document relationship from a genuinely cross-document one for the
    /// Citation Graph view. Idempotent -- recording the same
    /// (node_id, document_id) pair again (e.g. re-indexing an unchanged
    /// document) is a no-op, not a duplicate row.
    fn record_node_source(
        &self,
        node_id: ConceptNodeId,
        document_id: atlas_types::ids::DocumentId,
    ) -> Result<(), AppError>;

    /// All documents (by id) recorded as a source of this node via
    /// `record_node_source`.
    fn list_source_documents(
        &self,
        node_id: ConceptNodeId,
    ) -> Result<Vec<atlas_types::ids::DocumentId>, AppError>;
}
