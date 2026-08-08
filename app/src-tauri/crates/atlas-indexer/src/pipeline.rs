//! Indexing pipeline orchestration (§17, §18, §22, §35, §36). Wires the
//! Parser Layer (§36), the OCR Engine (§17), the Chunking Engine
//! (`chunker.rs`), the Embedding Engine (`embedding.rs`), and the AI Cache
//! repositories (§7.2) into a single `index_document` call:
//!
//! ```text
//! File change detected (Watcher, §21)
//!    -> file type detection (extension, §36.1)
//!    -> Parser Selector resolves a Parser (§36.1) -> ParsedDocument (§35.1)
//!    -> image-only blocks are OCR'd (§17: only where detected, not assumed)
//!    -> Chunking Engine splits blocks into Chunks (§18, §33.3)
//!    -> Embedding Engine embeds each chunk (§18)
//!    -> Chunk + vector + embedding-pointer persisted to AI Cache (§7.2)
//! ```
//!
//! Cache invalidation (§22): if the file's content hash matches the
//! document's `last_indexed_hash`, indexing is skipped entirely -- this is
//! what makes re-scans of an unchanged workspace cheap and what makes the
//! Folder Watcher's `FileUpdated` events safe to fire generously.

use std::sync::Arc;

use atlas_config::SettingsProvider;
use atlas_events::EventBus;
use atlas_types::document::DocumentRecord;
use atlas_types::document::{ParseStatus, ParsedDocument};
use atlas_types::event::{AppEvent, EventType};
use atlas_types::ids::{ChunkId, DocumentId, WorkspaceId};
use atlas_utils::hashing::hash_bytes;
use atlas_utils::paths::extension_lower;
use atlas_utils::time::now_iso8601;
use atlas_utils::AppError;

use crate::chunker::{chunk_document, ChunkingConfig};
use crate::embedding::EmbeddingEngine;
use crate::ocr::{requires_ocr, OcrEngine};
use crate::vector_search::VectorStore;
use crate::{ChunkRepository, DocumentRepository, EmbeddingRepository, ParserSelector};

pub struct IndexingPipeline {
    documents: Arc<dyn DocumentRepository>,
    chunks: Arc<dyn ChunkRepository>,
    parsers: Arc<ParserSelector>,
    settings: Arc<dyn SettingsProvider>,
    events: Arc<dyn EventBus>,
    ocr: Arc<dyn OcrEngine>,
    embedder: Arc<dyn EmbeddingEngine>,
    embeddings: Arc<dyn EmbeddingRepository>,
    vector_store: Arc<dyn VectorStore>,
    chunking_config: ChunkingConfig,
}

/// Outcome of a single `index_document` call, surfaced back to the caller
/// (a Background Job worker, or a direct IPC call for "reprocess now",
/// §43.1 `ocr.reprocess`) so it knows whether real work happened (§22:
/// "regenerated on next indexing pass, not eagerly").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexOutcome {
    /// Content hash unchanged since last index -- nothing to do (§22).
    Skipped,
    /// Parsed, chunked, embedded, and persisted.
    Indexed { chunk_count: usize },
}

impl IndexingPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        documents: Arc<dyn DocumentRepository>,
        chunks: Arc<dyn ChunkRepository>,
        parsers: Arc<ParserSelector>,
        settings: Arc<dyn SettingsProvider>,
        events: Arc<dyn EventBus>,
        ocr: Arc<dyn OcrEngine>,
        embedder: Arc<dyn EmbeddingEngine>,
        embeddings: Arc<dyn EmbeddingRepository>,
        vector_store: Arc<dyn VectorStore>,
    ) -> Self {
        Self {
            documents,
            chunks,
            parsers,
            settings,
            events,
            ocr,
            embedder,
            embeddings,
            vector_store,
            chunking_config: ChunkingConfig::default(),
        }
    }

    pub fn with_chunking_config(mut self, config: ChunkingConfig) -> Self {
        self.chunking_config = config;
        self
    }

    pub fn documents(&self) -> &Arc<dyn DocumentRepository> {
        &self.documents
    }

    pub fn chunks(&self) -> &Arc<dyn ChunkRepository> {
        &self.chunks
    }

    pub fn parsers(&self) -> &Arc<ParserSelector> {
        &self.parsers
    }

    pub fn settings(&self) -> &Arc<dyn SettingsProvider> {
        &self.settings
    }

    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    /// Index (or re-index) a single file (§17, §18, §36.1). `absolute_path`
    /// is the real filesystem path (already resolved from the workspace
    /// root + `relative_path` by the caller, e.g. via `safe_join`).
    pub fn index_document(
        &self,
        workspace_id: WorkspaceId,
        relative_path: &str,
        absolute_path: &str,
    ) -> Result<IndexOutcome, AppError> {
        let bytes = std::fs::read(absolute_path)
            .map_err(|e| AppError::indexing(format!("failed to read '{absolute_path}': {e}")))?;
        let content_hash = hash_bytes(&bytes);
        let size = bytes.len() as u64;

        let existing = self
            .documents
            .list_for_workspace(workspace_id)?
            .into_iter()
            .find(|d| d.relative_path == relative_path);

        // §22: cache invalidation key is content hash (+ parser/engine
        // version, carried on each Chunk's `parser_version`). If the file
        // hasn't changed since the last successful index, skip entirely.
        if let Some(existing) = &existing {
            if existing.last_indexed_hash.as_deref() == Some(content_hash.as_str()) {
                return Ok(IndexOutcome::Skipped);
            }
        }

        let file_type = extension_lower(std::path::Path::new(absolute_path))
            .ok_or_else(|| AppError::indexing(format!("'{relative_path}' has no file extension")))?;
        let file_type = normalize_file_type(&file_type);

        let parser = self
            .parsers
            .resolve(&file_type)
            .ok_or_else(|| AppError::indexing(format!("no parser registered for '{file_type}'")))?;

        let document_id = existing.as_ref().map(|d| d.id).unwrap_or(DocumentId(0));

        let record = DocumentRecord {
            id: document_id,
            workspace_id,
            relative_path: relative_path.to_string(),
            content_hash: content_hash.clone(),
            file_type: file_type.clone(),
            size,
            mtime: now_iso8601(),
            parse_status: ParseStatus::Parsing,
            last_indexed_hash: existing.as_ref().and_then(|d| d.last_indexed_hash.clone()),
            // Carried forward until this re-parse completes and (possibly)
            // overwrites it below -- avoids a brief window where a
            // previously-known authored date disappears from the Timeline
            // just because a re-index is in progress.
            authored_at: existing.as_ref().and_then(|d| d.authored_at.clone()),
        };
        let record = self.documents.upsert(record)?;

        match self.run_pipeline(workspace_id, record.id, absolute_path, parser) {
            Ok((chunk_count, authored_at)) => {
                let mut done = record.clone();
                // Fix 5 (P1 audit): a pipeline run that completes without
                // error but produces zero chunks (corrupt file, an
                // unsupported encoding the parser explicitly declines
                // rather than guesses at, etc.) must not look identical to
                // a real successful index -- that was a silent failure
                // mode with nothing but a backend log line to notice it by.
                done.parse_status = if chunk_count == 0 {
                    ParseStatus::ParsedEmpty
                } else {
                    ParseStatus::Parsed
                };
                done.last_indexed_hash = Some(content_hash);
                // Research Mode Timeline: overwrite with this parse's
                // finding -- `None` genuinely means "re-parsed and still
                // found no authored-date evidence" and should clear a
                // stale value (e.g. the document's front matter was
                // edited to remove the date), not leave a now-wrong one
                // in place.
                done.authored_at = authored_at;
                self.documents.upsert(done)?;

                self.events.publish(AppEvent {
                    id: None,
                    event_type: EventType::IndexCompleted,
                    payload: serde_json::json!({
                        "workspace_id": workspace_id.0,
                        "document_id": record.id.0,
                        "relative_path": relative_path,
                        "chunk_count": chunk_count,
                    }),
                    occurred_at: now_iso8601(),
                })?;

                Ok(IndexOutcome::Indexed { chunk_count })
            }
            Err(err) => {
                // §17/§21/§45.1: a per-file indexing failure is
                // Recoverable -- it must not halt indexing of the rest of
                // the workspace, but it must not be silently swallowed
                // either (§45.2). Record it on the document row and
                // publish `JobFailed` so it surfaces in the UI (§24: "per-
                // file, visible as a badge in the Sidebar").
                let mut failed = record.clone();
                failed.parse_status = ParseStatus::Failed;
                self.documents.upsert(failed)?;

                self.events.publish(AppEvent {
                    id: None,
                    event_type: EventType::JobFailed,
                    payload: serde_json::json!({
                        "workspace_id": workspace_id.0,
                        "document_id": record.id.0,
                        "relative_path": relative_path,
                        "error": err.message.clone(),
                    }),
                    occurred_at: now_iso8601(),
                })?;

                Err(err)
            }
        }
    }

    /// Parse -> OCR-fill -> chunk -> embed -> persist, for a document whose
    /// `documents` row already exists (§33.2, §33.3, §33.4). Returns the
    /// number of chunks written and the parser's best-effort authored-date
    /// finding (§ Research Mode Timeline; `None` when no parser-level
    /// evidence was found, never fabricated from `mtime`).
    fn run_pipeline(
        &self,
        workspace_id: WorkspaceId,
        document_id: DocumentId,
        absolute_path: &str,
        parser: &dyn crate::parser::Parser,
    ) -> Result<(usize, Option<String>), AppError> {
        let mut parsed: ParsedDocument = parser.parse(absolute_path)?;

        // §17: OCR runs only on blocks detected as image-based, never
        // assumed. Failure to OCR one block is recorded on the block's
        // text (left empty) rather than aborting the whole document
        // (§45.1 Recoverable) -- the rest of the document still indexes.
        for (block_index, block) in parsed.blocks.iter_mut().enumerate() {
            if requires_ocr(block) {
                // BUG FIX: this used to unconditionally read the whole
                // source file and hand it to the OCR engine as "the
                // image" -- for a multi-page PDF that's the entire raw
                // PDF binary, not a real image, which no OCR engine
                // (Tesseract or a vision model) can meaningfully read.
                // Prefer the parser's real per-block image when it can
                // provide one (`PdfParser` now rasterizes/extracts the
                // actual embedded page image, Fix 3 P0 audit); only fall
                // back to whole-file bytes for a parser that never
                // overrides `extract_ocr_image` at all (correct for
                // `ImageParser`, where the whole file already IS the
                // image). A parser that DOES override it
                // (`supports_ocr_image_extraction() == true`) returning
                // `None` means a genuine per-block extraction/render
                // failure -- that must NOT silently fall through to raw
                // whole-file bytes a second time (that both defeats the
                // point of the real extraction and would hand the OCR
                // engine garbage it can't decode either), so it's logged
                // and this block's OCR is skipped (text stays empty; the
                // rest of the document still indexes, per §45.1
                // Recoverable).
                let image_bytes = match parser.extract_ocr_image(absolute_path, block_index) {
                    Some(bytes) => Some(bytes),
                    None if parser.supports_ocr_image_extraction() => {
                        atlas_utils::log_info!(
                            "[IndexingPipeline] OCR image extraction failed for block {} of '{}' (parser reported a real per-block implementation); treating as a clean OCR failure for this block, not falling back to whole-file bytes",
                            block_index,
                            absolute_path
                        );
                        None
                    }
                    None => std::fs::read(absolute_path).ok(),
                };
                if let Some(image_bytes) = image_bytes {
                    if let Ok(text) = self.ocr.extract_text(&image_bytes) {
                        block.text_content = text;
                    }
                }
            }
        }

        let chunks = chunk_document(document_id, &parsed, self.chunking_config);

        // Incremental re-indexing (§22): the old chunk set for this
        // document is fully superseded by the new parse -- clear then
        // rewrite, keeping the AI Cache consistent with the current file
        // content rather than accumulating stale chunks from a previous
        // version.
        self.chunks.delete_for_document(document_id)?;

        let mut count = 0usize;
        for chunk in chunks {
            let inserted = self.chunks.insert(chunk)?;
            self.embed_and_store(workspace_id, inserted.document_id, inserted.id, &inserted.text_content)?;
            count += 1;
        }

        Ok((count, parsed.metadata.authored_at))
    }

    fn embed_and_store(
        &self,
        workspace_id: WorkspaceId,
        _document_id: DocumentId,
        chunk_id: ChunkId,
        text: &str,
    ) -> Result<(), AppError> {
        // TEMPORARY TRACE LOGGING (remove once the pipeline is confirmed working).
        if text.trim().is_empty() {
            atlas_utils::log_info!("[IndexingPipeline] embed_and_store skipped chunk_id={} (empty text)", chunk_id.0);
            return Ok(());
        }
        let vector = self.embedder.embed(text)?;
        atlas_utils::log_info!("[IndexingPipeline] embedded chunk_id={} ({} chars -> {} dims)", chunk_id.0, text.len(), vector.len());
        // BUG FIX (was hardcoded WorkspaceId(0), traced live: chat's
        // Retriever queried the real workspace id and got
        // "vector_search returned 0 raw hits" every time, because every
        // embedding had been namespaced under workspace 0 regardless of
        // which workspace it actually belonged to -- see §22 "workspace
        // namespacing" and the `VectorStore` trait doc on `upsert_vector`.
        let vector_id = self
            .vector_store
            .upsert_vector(workspace_id, chunk_id, vector)?;
        atlas_utils::log_info!("[IndexingPipeline] vector_store.upsert_vector workspace_id={} chunk_id={} vector_id={:?}", workspace_id.0, chunk_id.0, vector_id);
        self.embeddings.upsert(atlas_types::chunk::EmbeddingMetadata {
            chunk_id,
            vector_db_collection: "default".to_string(),
            vector_id,
            embedding_provider_id: self.embedder.provider_id(),
            created_at: now_iso8601(),
        })?;
        Ok(())
    }
}

/// Map a file extension to the `file_type` string the Parser Selector
/// registers parsers under (§36.1). Kept as a small, explicit table rather
/// than assuming extension == parser key everywhere, since several
/// extensions (jpg/jpeg/png/...) all resolve to the single "image" parser.
fn normalize_file_type(extension: &str) -> String {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" => "image".to_string(),
        "htm" => "html".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_config::hierarchy::LayeredSettingsProvider;
    use atlas_events::InMemoryEventBus;
    use atlas_types::ids::WorkspaceId;
    use std::sync::Mutex;

    use crate::embedding::HashEmbeddingEngine;
    use crate::ocr::NoopOcrEngine;
    use crate::parser::default_parser_selector;
    use crate::testing::{
        InMemoryChunkRepository, InMemoryDocumentRepository, InMemoryEmbeddingRepository,
    };

    struct InMemoryVectorStore {
        vectors: Mutex<Vec<(ChunkId, crate::embedding::Embedding)>>,
    }

    impl InMemoryVectorStore {
        fn new() -> Self {
            Self {
                vectors: Mutex::new(Vec::new()),
            }
        }
    }

    impl VectorStore for InMemoryVectorStore {
        fn upsert_vector(
            &self,
            _workspace_id: WorkspaceId,
            chunk_id: ChunkId,
            vector: crate::embedding::Embedding,
        ) -> Result<String, AppError> {
            self.vectors.lock().unwrap().push((chunk_id, vector));
            Ok(format!("vec-{}", chunk_id.0))
        }

        fn delete_vector(&self, _workspace_id: WorkspaceId, chunk_id: ChunkId) -> Result<(), AppError> {
            self.vectors
                .lock()
                .unwrap()
                .retain(|(id, _)| *id != chunk_id);
            Ok(())
        }
    }

    fn pipeline() -> IndexingPipeline {
        IndexingPipeline::new(
            Arc::new(InMemoryDocumentRepository::new()),
            Arc::new(InMemoryChunkRepository::new()),
            Arc::new(default_parser_selector()),
            Arc::new(LayeredSettingsProvider::new()),
            Arc::new(InMemoryEventBus::new()),
            Arc::new(NoopOcrEngine),
            Arc::new(HashEmbeddingEngine::default()),
            Arc::new(InMemoryEmbeddingRepository::new()),
            Arc::new(InMemoryVectorStore::new()),
        )
    }

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "atlas-pipeline-test-{name}-{}-{:?}.md",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn pipeline_exposes_all_injected_dependencies() {
        let pipeline = pipeline();
        assert!(pipeline
            .documents()
            .list_for_workspace(WorkspaceId(1))
            .unwrap()
            .is_empty());
        assert!(pipeline.parsers().resolve("pdf").is_some());
    }

    #[test]
    fn index_document_parses_chunks_and_embeds_a_markdown_file() {
        let pipeline = pipeline();
        let path = temp_file("md", "# Title\n\nSome content about gradients and loss.");

        let outcome = pipeline
            .index_document(WorkspaceId(1), "notes.md", path.to_str().unwrap())
            .unwrap();

        match outcome {
            IndexOutcome::Indexed { chunk_count } => assert!(chunk_count >= 1),
            IndexOutcome::Skipped => panic!("expected a fresh file to be indexed"),
        }

        let docs = pipeline
            .documents()
            .list_for_workspace(WorkspaceId(1))
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].parse_status, ParseStatus::Parsed);

        let chunks = pipeline.chunks().list_for_document(docs[0].id).unwrap();
        assert!(!chunks.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    /// Fix 5 (P1 audit): a document that parses without error but yields
    /// zero chunks (here: an empty file, so the Markdown parser produces
    /// no blocks at all) must be distinguishable from a real successful
    /// index, not silently indistinguishable from `Parsed`.
    #[test]
    fn zero_chunk_result_is_recorded_as_parsed_empty_not_parsed() {
        let pipeline = pipeline();
        let path = temp_file("empty", "");

        let outcome = pipeline
            .index_document(WorkspaceId(1), "empty.md", path.to_str().unwrap())
            .unwrap();

        match outcome {
            IndexOutcome::Indexed { chunk_count } => assert_eq!(chunk_count, 0),
            IndexOutcome::Skipped => panic!("expected a fresh file to be indexed"),
        }

        let docs = pipeline
            .documents()
            .list_for_workspace(WorkspaceId(1))
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].parse_status, ParseStatus::ParsedEmpty);
        assert_ne!(docs[0].parse_status, ParseStatus::Parsed);
        assert_ne!(docs[0].parse_status, ParseStatus::Failed);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reindexing_unchanged_file_is_skipped() {
        let pipeline = pipeline();
        let path = temp_file("md-skip", "# Title\n\nUnchanging content.");

        pipeline
            .index_document(WorkspaceId(1), "same.md", path.to_str().unwrap())
            .unwrap();
        let second = pipeline
            .index_document(WorkspaceId(1), "same.md", path.to_str().unwrap())
            .unwrap();

        assert_eq!(second, IndexOutcome::Skipped);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn changing_file_content_triggers_reindex_and_replaces_chunks() {
        let pipeline = pipeline();
        let path = temp_file("md-change", "# Title\n\nOriginal content here.");

        pipeline
            .index_document(WorkspaceId(1), "changing.md", path.to_str().unwrap())
            .unwrap();
        std::fs::write(&path, "# Title\n\nCompletely different content now.").unwrap();
        let outcome = pipeline
            .index_document(WorkspaceId(1), "changing.md", path.to_str().unwrap())
            .unwrap();

        assert!(matches!(outcome, IndexOutcome::Indexed { .. }));
        let docs = pipeline
            .documents()
            .list_for_workspace(WorkspaceId(1))
            .unwrap();
        let chunks = pipeline.chunks().list_for_document(docs[0].id).unwrap();
        assert!(chunks
            .iter()
            .any(|c| c.text_content.contains("Completely")));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_a_recoverable_indexing_error() {
        let pipeline = pipeline();
        let err = pipeline
            .index_document(WorkspaceId(1), "missing.md", "/no/such/path.md")
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::Recoverable);
    }

    #[test]
    fn unrecognized_extension_is_a_recoverable_indexing_error() {
        let pipeline = pipeline();
        let path = std::env::temp_dir().join(format!(
            "atlas-pipeline-test-weird-{}-{:?}.xyz",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, "content").unwrap();
        let err = pipeline
            .index_document(WorkspaceId(1), "weird.xyz", path.to_str().unwrap())
            .unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::Recoverable);
        let _ = std::fs::remove_file(&path);
    }
}
