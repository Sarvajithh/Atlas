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
    /// Mirrors `ParsedDocument::metadata::authored_at` (Research Mode
    /// Timeline). Persisted separately from the ephemeral `ParsedDocument`
    /// each re-parse produces, so the Timeline can query it without
    /// re-parsing every document on every view. `None` until a parse
    /// finds genuine authored-date evidence.
    pub authored_at: Option<String>,
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
    /// Best-effort publication/authored date, as `YYYY-MM-DD`, distinct
    /// from the filesystem `mtime` on `DocumentRecord` below (§ Research
    /// Mode Timeline). `None` when no parser could find genuine authored-
    /// date evidence -- never filled from `mtime` or a re-index/re-save
    /// time, since that would be actively misleading (a re-saved older
    /// paper sorting as "recent"). See `atlas_indexer::parser::dates` for
    /// where this actually gets populated.
    pub authored_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub metadata: DocumentMetadata,
    pub blocks: Vec<Block>,
}
