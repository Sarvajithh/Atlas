//! Citation Graph queries (§20 Research Mode phase). Derives cross-
//! document relationships from the existing Concept Graph edges + the
//! node-provenance data `extraction.rs` now records -- queries, doesn't
//! duplicate storage, and doesn't touch the frozen `concept_nodes`/
//! `concept_edges` schema.
//!
//! "Cross-document" here means: the two concepts an edge connects are, in
//! total, sourced from more than one distinct document (§ objective
//! "citation graph / cross-document linking"). A pair of concepts that
//! only ever co-occur within a single document's extraction is a real
//! edge but not a *citation* -- there's no cross-source link to show.

use std::collections::HashSet;
use std::sync::Arc;

use atlas_types::concept::{ConceptEdge, ConceptNode};
use atlas_types::ids::{ConceptNodeId, DocumentId, WorkspaceId};
use atlas_utils::AppError;

use crate::repository::GraphRepository;

/// One cross-document edge, with both endpoint nodes resolved and the
/// full set of documents (deduplicated, union of both endpoints') that
/// make this edge a genuinely cross-document relationship rather than a
/// within-one-document one.
#[derive(Debug, Clone)]
pub struct CrossDocumentEdge {
    pub edge: ConceptEdge,
    pub from_node: ConceptNode,
    pub to_node: ConceptNode,
    pub source_documents: Vec<DocumentId>,
}

/// Finds every Concept Graph edge across the given workspaces whose
/// endpoints are, between them, sourced from more than one document.
/// `workspace_ids` lets Research Mode span multiple workspaces at once
/// (§ objective "cross-document/cross-workspace context"); a single-
/// element slice scopes it to one workspace, same as everywhere else.
///
/// No mock/fabricated relationships (§ objective): every edge returned
/// traces to a real row in `concept_edges`, and every `source_documents`
/// entry traces to a real `concept_node_sources` row recorded by
/// extraction -- nothing here is synthesized.
pub fn list_cross_document_edges(
    repository: &Arc<dyn GraphRepository>,
    workspace_ids: &[WorkspaceId],
) -> Result<Vec<CrossDocumentEdge>, AppError> {
    let mut results = Vec::new();
    let mut seen_edge_ids = HashSet::new();

    for &workspace_id in workspace_ids {
        let nodes = repository.list_nodes_for_workspace(workspace_id)?;
        for node in &nodes {
            for edge in repository.list_edges_for_node(node.id)? {
                if !seen_edge_ids.insert(edge.id) {
                    continue; // already produced via the other endpoint
                }

                let (Some(from_node), Some(to_node)) =
                    (resolve(repository, edge.from_node_id, &nodes)?, resolve(repository, edge.to_node_id, &nodes)?)
                else {
                    // An endpoint outside the requested workspace set (or
                    // somehow missing) -- skip rather than show a half
                    // resolved edge.
                    continue;
                };

                let mut source_documents: Vec<DocumentId> = repository.list_source_documents(from_node.id)?;
                source_documents.extend(repository.list_source_documents(to_node.id)?);
                source_documents.sort_by_key(|d| d.0);
                source_documents.dedup();

                if source_documents.len() > 1 {
                    results.push(CrossDocumentEdge {
                        edge,
                        from_node,
                        to_node,
                        source_documents,
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Resolve a node id against the already-fetched `nodes` for the current
/// workspace first (avoids a repository round-trip for the common case of
/// both endpoints being in the same workspace), falling back to a direct
/// lookup for the cross-workspace case.
fn resolve(
    repository: &Arc<dyn GraphRepository>,
    id: ConceptNodeId,
    nodes: &[ConceptNode],
) -> Result<Option<ConceptNode>, AppError> {
    if let Some(found) = nodes.iter().find(|n| n.id == id) {
        return Ok(Some(found.clone()));
    }
    repository.find_node(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::InMemoryGraphRepository;
    use atlas_types::concept::RelationType;
    use atlas_types::ids::ConceptEdgeId;

    fn node(repo: &InMemoryGraphRepository, workspace_id: i64, label: &str) -> ConceptNode {
        repo.insert_node(ConceptNode {
            id: ConceptNodeId(0),
            workspace_id: WorkspaceId(workspace_id),
            label: label.to_string(),
            description: None,
            created_at: "t".to_string(),
        })
        .unwrap()
    }

    #[test]
    fn an_edge_sourced_from_two_documents_is_cross_document() {
        let repo = InMemoryGraphRepository::new();
        let a = node(&repo, 1, "A");
        let b = node(&repo, 1, "B");
        repo.record_node_source(a.id, DocumentId(1)).unwrap();
        repo.record_node_source(b.id, DocumentId(2)).unwrap();
        repo.insert_edge(ConceptEdge {
            id: ConceptEdgeId(0),
            from_node_id: a.id,
            to_node_id: b.id,
            relation_type: RelationType::RelatedTo,
            weight: 1.0,
        })
        .unwrap();

        let repository: Arc<dyn GraphRepository> = Arc::new(repo);
        let edges = list_cross_document_edges(&repository, &[WorkspaceId(1)]).unwrap();

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_documents, vec![DocumentId(1), DocumentId(2)]);
    }

    #[test]
    fn an_edge_sourced_from_only_one_document_is_not_cross_document() {
        let repo = InMemoryGraphRepository::new();
        let a = node(&repo, 1, "A");
        let b = node(&repo, 1, "B");
        repo.record_node_source(a.id, DocumentId(1)).unwrap();
        repo.record_node_source(b.id, DocumentId(1)).unwrap();
        repo.insert_edge(ConceptEdge {
            id: ConceptEdgeId(0),
            from_node_id: a.id,
            to_node_id: b.id,
            relation_type: RelationType::RelatedTo,
            weight: 1.0,
        })
        .unwrap();

        let repository: Arc<dyn GraphRepository> = Arc::new(repo);
        let edges = list_cross_document_edges(&repository, &[WorkspaceId(1)]).unwrap();

        assert!(edges.is_empty());
    }

    #[test]
    fn each_edge_is_returned_only_once_even_though_both_endpoints_are_scanned() {
        let repo = InMemoryGraphRepository::new();
        let a = node(&repo, 1, "A");
        let b = node(&repo, 1, "B");
        repo.record_node_source(a.id, DocumentId(1)).unwrap();
        repo.record_node_source(b.id, DocumentId(2)).unwrap();
        repo.insert_edge(ConceptEdge {
            id: ConceptEdgeId(0),
            from_node_id: a.id,
            to_node_id: b.id,
            relation_type: RelationType::RelatedTo,
            weight: 1.0,
        })
        .unwrap();

        let repository: Arc<dyn GraphRepository> = Arc::new(repo);
        let edges = list_cross_document_edges(&repository, &[WorkspaceId(1)]).unwrap();

        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn spans_multiple_requested_workspaces() {
        let repo = InMemoryGraphRepository::new();
        let a = node(&repo, 1, "A");
        let b = node(&repo, 2, "B");
        repo.record_node_source(a.id, DocumentId(1)).unwrap();
        repo.record_node_source(b.id, DocumentId(2)).unwrap();
        repo.insert_edge(ConceptEdge {
            id: ConceptEdgeId(0),
            from_node_id: a.id,
            to_node_id: b.id,
            relation_type: RelationType::RelatedTo,
            weight: 1.0,
        })
        .unwrap();

        let repository: Arc<dyn GraphRepository> = Arc::new(repo);
        // Only requesting workspace 1 won't find node b's edge iteration
        // (edges are discovered by scanning each requested workspace's own
        // nodes), so both workspaces must be passed for a genuinely
        // cross-workspace edge to surface.
        let edges = list_cross_document_edges(&repository, &[WorkspaceId(1), WorkspaceId(2)]).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn no_edges_is_an_empty_result_not_an_error() {
        let repo = InMemoryGraphRepository::new();
        let repository: Arc<dyn GraphRepository> = Arc::new(repo);
        let edges = list_cross_document_edges(&repository, &[WorkspaceId(1)]).unwrap();
        assert!(edges.is_empty());
    }
}
