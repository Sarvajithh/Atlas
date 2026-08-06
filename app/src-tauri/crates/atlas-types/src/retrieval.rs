//! Hybrid retrieval / citation shapes (§18, §39.1 "citation preparation",
//! §44.1 "Shared Location Reference"). Plain DTOs only, per this crate's
//! "shapes only" rule (§11) -- the Retriever/Reranker/Context Builder logic
//! that produces these lives in `atlas-models` (§14.1); the keyword/vector
//! search adapters that feed it live in `atlas-db`/`atlas-vector` (§18).

use serde::{Deserialize, Serialize};

use crate::ids::{ChunkId, DocumentId};

/// One candidate produced by keyword search, vector search, or the merged
/// hybrid result (§18). `score` is retriever-specific (BM25-ish lexical
/// overlap or cosine similarity) until the Reranker (§14.1) normalizes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub text_content: String,
    pub page_or_location_ref: String,
    pub score: f32,
}

/// One ranked result from Global Search (§9, §43.1 `search.global`): unlike
/// `SearchHit` (an internal retrieval candidate scoped to a single
/// workspace), this is the display-ready shape the frontend renders and
/// navigates from -- it carries which workspace the hit came from (needed
/// for cross-workspace "All" scope, §9) and the document's `relative_path`
/// so a result can be shown/opened without a second round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSearchResult {
    pub document_id: DocumentId,
    pub workspace_id: crate::ids::WorkspaceId,
    pub workspace_name: String,
    pub chunk_id: ChunkId,
    pub relative_path: String,
    pub snippet: String,
    pub location_ref: String,
    pub score: f32,
}

/// A citation pointing back into a source document (§39.1, §44.1), attached
/// to assembled context so an eventual answer can cite its sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub document_id: DocumentId,
    pub chunk_id: ChunkId,
    pub location_ref: String,
    /// A short excerpt (not the full chunk) suitable for display next to
    /// the citation (§44.2 "Assistant -> Viewer" click target).
    pub snippet: String,
}
