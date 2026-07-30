//! Concept Graph shapes (§20, §33.5, §33.6).

use serde::{Deserialize, Serialize};

use crate::ids::{ConceptEdgeId, ConceptNodeId, WorkspaceId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    pub id: ConceptNodeId,
    pub workspace_id: WorkspaceId,
    pub label: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationType {
    PrerequisiteOf,
    RelatedTo,
    PartOf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptEdge {
    pub id: ConceptEdgeId,
    pub from_node_id: ConceptNodeId,
    pub to_node_id: ConceptNodeId,
    pub relation_type: RelationType,
    pub weight: f32,
}
