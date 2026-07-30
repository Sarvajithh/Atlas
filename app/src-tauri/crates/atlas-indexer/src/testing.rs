//! Testing infrastructure for this crate (§30): dependency-free, in-memory
//! implementations of the repository interfaces defined here. These are
//! pure storage -- no OCR, parsing, or pipeline orchestration logic, which
//! remain out of scope for this task ("DO NOT IMPLEMENT ... Document
//! Pipeline ... OCR").

use std::sync::Mutex;

use atlas_types::chunk::{Chunk, EmbeddingMetadata};
use atlas_types::document::DocumentRecord;
use atlas_types::ids::{ChunkId, DocumentId, WorkspaceId};
use atlas_utils::AppError;

use crate::{ChunkRepository, DocumentRepository, EmbeddingRepository};

fn lock_err(what: &str) -> AppError {
    AppError::user(format!("{what} lock poisoned"))
}

#[derive(Default)]
pub struct InMemoryDocumentRepository {
    documents: Mutex<Vec<DocumentRecord>>,
}

impl InMemoryDocumentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DocumentRepository for InMemoryDocumentRepository {
    fn find_by_id(&self, id: DocumentId) -> Result<Option<DocumentRecord>, AppError> {
        let docs = self.documents.lock().map_err(|_| lock_err("document"))?;
        Ok(docs.iter().find(|d| d.id == id).cloned())
    }

    fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<DocumentRecord>, AppError> {
        let docs = self.documents.lock().map_err(|_| lock_err("document"))?;
        Ok(docs
            .iter()
            .filter(|d| d.workspace_id == workspace_id)
            .cloned()
            .collect())
    }

    fn upsert(&self, document: DocumentRecord) -> Result<DocumentRecord, AppError> {
        let mut docs = self.documents.lock().map_err(|_| lock_err("document"))?;
        if let Some(existing) = docs.iter_mut().find(|d| d.id == document.id) {
            *existing = document.clone();
        } else {
            docs.push(document.clone());
        }
        Ok(document)
    }

    fn delete(&self, id: DocumentId) -> Result<(), AppError> {
        let mut docs = self.documents.lock().map_err(|_| lock_err("document"))?;
        docs.retain(|d| d.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryChunkRepository {
    chunks: Mutex<Vec<Chunk>>,
}

impl InMemoryChunkRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ChunkRepository for InMemoryChunkRepository {
    fn list_for_document(&self, document_id: DocumentId) -> Result<Vec<Chunk>, AppError> {
        let chunks = self.chunks.lock().map_err(|_| lock_err("chunk"))?;
        Ok(chunks
            .iter()
            .filter(|c| c.document_id == document_id)
            .cloned()
            .collect())
    }

    fn insert(&self, chunk: Chunk) -> Result<Chunk, AppError> {
        let mut chunks = self.chunks.lock().map_err(|_| lock_err("chunk"))?;
        chunks.push(chunk.clone());
        Ok(chunk)
    }

    fn delete_for_document(&self, document_id: DocumentId) -> Result<(), AppError> {
        let mut chunks = self.chunks.lock().map_err(|_| lock_err("chunk"))?;
        chunks.retain(|c| c.document_id != document_id);
        Ok(())
    }

    fn find_by_id(&self, id: ChunkId) -> Result<Option<Chunk>, AppError> {
        let chunks = self.chunks.lock().map_err(|_| lock_err("chunk"))?;
        Ok(chunks.iter().find(|c| c.id == id).cloned())
    }
}

#[derive(Default)]
pub struct InMemoryEmbeddingRepository {
    metadata: Mutex<Vec<EmbeddingMetadata>>,
}

impl InMemoryEmbeddingRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EmbeddingRepository for InMemoryEmbeddingRepository {
    fn upsert(&self, metadata: EmbeddingMetadata) -> Result<(), AppError> {
        let mut items = self
            .metadata
            .lock()
            .map_err(|_| lock_err("embedding metadata"))?;
        if let Some(existing) = items.iter_mut().find(|m| m.chunk_id == metadata.chunk_id) {
            *existing = metadata;
        } else {
            items.push(metadata);
        }
        Ok(())
    }

    fn find_for_chunk(&self, chunk_id: ChunkId) -> Result<Option<EmbeddingMetadata>, AppError> {
        let items = self
            .metadata
            .lock()
            .map_err(|_| lock_err("embedding metadata"))?;
        Ok(items.iter().find(|m| m.chunk_id == chunk_id).cloned())
    }

    fn delete_for_chunk(&self, chunk_id: ChunkId) -> Result<(), AppError> {
        let mut items = self
            .metadata
            .lock()
            .map_err(|_| lock_err("embedding metadata"))?;
        items.retain(|m| m.chunk_id != chunk_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::document::ParseStatus;

    fn sample_document(id: i64, workspace_id: i64) -> DocumentRecord {
        DocumentRecord {
            id: DocumentId(id),
            workspace_id: WorkspaceId(workspace_id),
            relative_path: "notes/ch1.pdf".to_string(),
            content_hash: "abc123".to_string(),
            file_type: "pdf".to_string(),
            size: 1024,
            mtime: "1970-01-01T00:00:00Z".to_string(),
            parse_status: ParseStatus::Pending,
            last_indexed_hash: None,
        }
    }

    fn sample_chunk(id: i64, document_id: i64) -> Chunk {
        Chunk {
            id: ChunkId(id),
            document_id: DocumentId(document_id),
            sequence_index: 0,
            text_content: "text".to_string(),
            page_or_location_ref: "1".to_string(),
            token_count: 10,
            parser_version: "1".to_string(),
        }
    }

    #[test]
    fn document_repository_upsert_then_find() {
        let repo = InMemoryDocumentRepository::new();
        repo.upsert(sample_document(1, 1)).unwrap();
        assert!(repo.find_by_id(DocumentId(1)).unwrap().is_some());
    }

    #[test]
    fn document_repository_upsert_replaces_existing() {
        let repo = InMemoryDocumentRepository::new();
        repo.upsert(sample_document(1, 1)).unwrap();
        let mut updated = sample_document(1, 1);
        updated.content_hash = "def456".to_string();
        repo.upsert(updated).unwrap();

        assert_eq!(repo.list_for_workspace(WorkspaceId(1)).unwrap().len(), 1);
        assert_eq!(
            repo.find_by_id(DocumentId(1))
                .unwrap()
                .unwrap()
                .content_hash,
            "def456"
        );
    }

    #[test]
    fn document_repository_delete_removes_document() {
        let repo = InMemoryDocumentRepository::new();
        repo.upsert(sample_document(1, 1)).unwrap();
        repo.delete(DocumentId(1)).unwrap();
        assert!(repo.find_by_id(DocumentId(1)).unwrap().is_none());
    }

    #[test]
    fn chunk_repository_delete_for_document_clears_only_that_document() {
        let repo = InMemoryChunkRepository::new();
        repo.insert(sample_chunk(1, 1)).unwrap();
        repo.insert(sample_chunk(2, 2)).unwrap();
        repo.delete_for_document(DocumentId(1)).unwrap();

        assert!(repo.list_for_document(DocumentId(1)).unwrap().is_empty());
        assert_eq!(repo.list_for_document(DocumentId(2)).unwrap().len(), 1);
    }

    #[test]
    fn embedding_repository_upsert_then_find_for_chunk() {
        let repo = InMemoryEmbeddingRepository::new();
        repo.upsert(EmbeddingMetadata {
            chunk_id: ChunkId(1),
            vector_db_collection: "workspace-1".to_string(),
            vector_id: "v1".to_string(),
            embedding_provider_id: "embedding-engine".to_string(),
            created_at: "1970-01-01T00:00:00Z".to_string(),
        })
        .unwrap();

        assert!(repo.find_for_chunk(ChunkId(1)).unwrap().is_some());
    }
}
