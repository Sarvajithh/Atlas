//! Document and Document-Abstraction-Layer shapes (§33.2, §35).

use serde::{Deserialize, Serialize};

use crate::ids::{DocumentId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseStatus {
    Pending,
    Parsing,
    Parsed,
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
