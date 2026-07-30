//! atlas-indexer
//!
//! Indexing Module (§14): OCR Pipeline (§17), the Document Abstraction Layer
//! (§35), the Parser Layer (§36), and the Search Pipeline's indexing side
//! (§18). Parsers MUST NOT perform chunking, embedding, or retrieval logic
//! (§36.3) — those responsibilities are separated into their own modules
//! here, all behind interfaces.

pub mod chunk_repository;
pub mod document_repository;
pub mod embedding_repository;
pub mod job_repository;
pub mod job_queue;
pub mod ocr;
pub mod parser;
pub mod pipeline;
pub mod testing;

pub use chunk_repository::ChunkRepository;
pub use document_repository::DocumentRepository;
pub use embedding_repository::EmbeddingRepository;
pub use job_queue::JobQueue;
pub use job_repository::JobRepository;
pub use parser::{Parser, ParserSelector};
pub use testing::{
    InMemoryChunkRepository, InMemoryDocumentRepository, InMemoryEmbeddingRepository,
    InMemoryJobRepository,
};
