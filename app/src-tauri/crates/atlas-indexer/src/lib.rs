//! atlas-indexer
//!
//! Indexing Module (§14): OCR Pipeline (§17), the Document Abstraction Layer
//! (§35), the Parser Layer (§36), and the Search Pipeline's indexing side
//! (§18). Parsers MUST NOT perform chunking, embedding, or retrieval logic
//! (§36.3) — those responsibilities are separated into their own modules
//! here, all behind interfaces.

pub mod chunk_repository;
pub mod chunker;
pub mod document_repository;
pub mod embedding;
pub mod embedding_repository;
pub mod job_repository;
pub mod job_queue;
pub mod keyword_search;
pub mod ocr;
pub mod parser;
pub mod pipeline;
pub mod testing;
pub mod vector_search;

pub use chunk_repository::ChunkRepository;
pub use chunker::{chunk_document, ChunkingConfig};
pub use document_repository::DocumentRepository;
pub use embedding::{cosine_similarity, Embedding, EmbeddingEngine, HashEmbeddingEngine};
pub use embedding_repository::EmbeddingRepository;
pub use job_queue::JobQueue;
pub use job_repository::JobRepository;
pub use keyword_search::KeywordSearchRepository;
pub use ocr::OcrEngine;
pub use parser::{default_parser_selector, Parser, ParserSelector};
pub use testing::{
    InMemoryChunkRepository, InMemoryDocumentRepository, InMemoryEmbeddingRepository,
    InMemoryJobRepository,
};
pub use vector_search::{VectorSearchRepository, VectorStore};
