//! Chunk and embedding-metadata shapes (§33.3, §33.4).

use serde::{Deserialize, Serialize};

use crate::ids::{ChunkId, DocumentId};

/// Mirrors the `chunks` table (§33.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub sequence_index: u32,
    pub text_content: String,
    pub page_or_location_ref: String,
    pub token_count: u32,
    pub parser_version: String,
}

/// Mirrors the `embeddings_metadata` table (§33.4). The vector itself lives
/// in the Vector DB, never in SQLite (§5, §33.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub chunk_id: ChunkId,
    pub vector_db_collection: String,
    pub vector_id: String,
    pub embedding_provider_id: String,
    pub created_at: String,
}
