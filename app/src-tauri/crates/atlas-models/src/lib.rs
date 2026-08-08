//! atlas-models
//!
//! The Engines Module (§14.1): Model Registry (§37), Model Scheduler (§15),
//! Resource Manager (§38), Context Builder (§39), and Prompt Builder (§40).
//! Engines are referred to by role name everywhere in code, UI copy, and
//! docs -- never by underlying model name (§14.1, §27). Engine-to-model
//! assignment is data in the Model Registry, editable from Settings (§23),
//! never a compiled-in mapping (§37.2).

pub mod citation;
pub mod context_builder;
pub mod discovery;
pub mod embedding;
pub mod engine;
pub mod engines;
pub mod ollama;
pub mod prompt_builder;
pub mod registry;
pub mod reranker;
pub mod resource_manager;
pub mod retriever;
pub mod scheduler;
pub mod vision_ocr;

pub use citation::{citation_for_hit, citations_for_hits};
pub use context_builder::ContextBuilder;
pub use discovery::ModelDiscoveryService;
pub use embedding::OllamaEmbeddingEngine;
pub use engine::Engine;
pub use engines::{EnginePool, OllamaEngine};
pub use ollama::{DiscoveredModel, GenerationChunk, ModelCapability, OllamaConnection, OllamaProvider};
pub use prompt_builder::{PromptBuilder, ResearchPromptMode};
pub use registry::{InMemoryModelRegistry, ModelProvider, ModelRegistryRepository};
pub use vision_ocr::OllamaVisionOcrEngine;
pub use reranker::Reranker;
pub use resource_manager::ResourceManager;
pub use retriever::{HybridWeights, Retriever};
pub use scheduler::{Intent, ModelScheduler, RoutingTable};
