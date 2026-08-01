//! atlas-vector
//!
//! Vector DB client abstraction (§5, §14). Implements `EmbeddingRepository`
//! (owned by atlas-indexer) against an embedded/local vector store
//! (Qdrant or LanceDB, §5). Collections are namespaced per workspace (§22).

pub mod client;
pub mod store;

pub use client::VectorDbEmbeddingRepository;
pub use store::EmbeddedVectorStore;
