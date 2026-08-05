//! Document and Document-Abstraction-Layer shapes (§33.2, §35).

use serde::{Deserialize, Serialize};

use crate::ids::{DocumentId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseStatus {
    Pending,
    Parsing,
    Parsed,
    /// Fix 5 (P1 audit): the pipeline ran to completion with no error, but
    /// produced zero chunks (e.g. a corrupt file, an unsupported PDF
    /// encoding gap the parser explicitly declines rather than guesses at
    /// -- see the `pdf` parser's documented limitations -- or any other
    /// edge case). Distinct from `Parsed` (real content was indexed) and
    /// from `Failed` (the pipeline itself errored out) -- previously this
    /// state was indistinguishable from `Parsed`, so a document that
    /// silently produced no usable content looked identical, in the UI, to
    /// one that was successfully and completely indexed.
    ParsedEmpty,
    Failed,
}

/// Mirrors the `documents` table (§33.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: DocumentId,
    pub workspace_id: WorkspaceId,
    pub relative_path: String,
    pub content_hash: String,
    pub file_type: String,
    pub size: u64,
    pub mtime: String,
    pub parse_status: ParseStatus,
    pub last_indexed_hash: Option<String>,
}

/// A location reference into a source document, as described in §35.1 and
/// used by the Viewer Contract (§44) to keep selections/citations in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationRef {
    pub page_or_location: String,
}

/// The common internal representation every Parser (§36) produces (§35.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    Heading,
    Paragraph,
    Image,
    Table,
    Code,
    Equation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub block_type: BlockType,
    pub location_ref: LocationRef,
    pub text_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub title: String,
    pub file_type: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub metadata: DocumentMetadata,
    pub blocks: Vec<Block>,
}
