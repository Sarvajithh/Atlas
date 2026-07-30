//! SQLite-backed `ChunkRepository` (§33.3).

use atlas_indexer::ChunkRepository;
use atlas_types::chunk::Chunk;
use atlas_types::ids::{ChunkId, DocumentId};
use atlas_utils::AppError;

use crate::connection::SqliteConnection;

pub struct SqliteChunkRepository {
    connection: SqliteConnection,
}

impl SqliteChunkRepository {
    pub fn new(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl ChunkRepository for SqliteChunkRepository {
    fn list_for_document(&self, _document_id: DocumentId) -> Result<Vec<Chunk>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn insert(&self, _chunk: Chunk) -> Result<Chunk, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn delete_for_document(&self, _document_id: DocumentId) -> Result<(), AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }

    fn find_by_id(&self, _id: ChunkId) -> Result<Option<Chunk>, AppError> {
        unimplemented!("SQLite query implementation is out of scope for this milestone")
    }
}
