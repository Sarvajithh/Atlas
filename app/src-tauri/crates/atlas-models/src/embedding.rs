//! Ollama-backed Embedding Engine (§14.1, §18, §37.1). `atlas_indexer::embedding`
//! defines the `EmbeddingEngine` trait and documents that the concrete
//! backend "is resolved through the Model Registry (owned by atlas-models)
//! and injected here" -- this is that concrete backend, replacing
//! `HashEmbeddingEngine` in production wiring (`atlas-core::facade`).
//!
//! Unlike `OllamaVisionOcrEngine` (§17), this engine does NOT fall back to
//! a fake/local implementation on failure: a hashed bag-of-words vector and
//! a real semantic embedding are not interchangeable in a vector index --
//! silently mixing them would corrupt every future similarity search over
//! that workspace with no way to detect it later. "Never use fake
//! embeddings" (Part 1) means every failure here is surfaced as a
//! `ModelError` (§45.1) instead: Recoverable at the file/chunk level (the
//! indexing pipeline already treats one failed job as isolated, §21), never
//! silently degraded.

use std::sync::Arc;

use atlas_indexer::embedding::{Embedding, EmbeddingEngine};
use atlas_types::model::EngineRole;
use atlas_utils::AppError;

use crate::ollama::OllamaProvider;
use crate::registry::ModelRegistryRepository;

/// The SAME model generates both document and query embeddings (README
/// "Preferred Embedding" responsibilities: "The SAME model must generate
/// both document and query embeddings"). That invariant falls out for free
/// here because both `embed()` and `embed_batch()` resolve the model from
/// the same place, per call: whatever the Model Registry currently has
/// selected for `EngineRole::Embedding` (§37.2 -- assignment is data, never
/// hardcoded). If the assignment changes mid-session (user reassigns in
/// Settings, §23), the *next* call picks up the new model; already-indexed
/// vectors from a prior model are not silently reused as if compatible --
/// callers are expected to treat an embedding-model change as a cache
/// invalidation event (§22), same as any other engine/version change.
pub struct OllamaEmbeddingEngine {
    ollama: Arc<OllamaProvider>,
    model_registry: Arc<dyn ModelRegistryRepository>,
    /// Reported to callers via `dimensions()`. Not used to validate or
    /// reshape vectors returned by Ollama -- it exists purely so callers
    /// that need to size storage up front (e.g. the vector store, §5) have
    /// a number, consistent with the `EmbeddingEngine` trait's contract.
    /// Configured (constructor argument), never hardcoded (§46.1); the
    /// composition root (`atlas-core::facade`) is expected to set this from
    /// the currently-assigned embedding model's known output size, or a
    /// sane default which the first real embedding response then confirms.
    dimensions: usize,
}

impl OllamaEmbeddingEngine {
    pub fn new(ollama: Arc<OllamaProvider>, model_registry: Arc<dyn ModelRegistryRepository>, dimensions: usize) -> Self {
        Self {
            ollama,
            model_registry,
            dimensions: dimensions.max(1),
        }
    }

    /// Resolve the model currently assigned to `EngineRole::Embedding`
    /// (§37.1: "give me the current model for X", never a hardcoded name).
    /// A missing assignment is a `ModelError`, not a silent fallback --
    /// there is no safe fake embedding to fall back to (see module docs).
    fn resolve_model(&self) -> Result<String, AppError> {
        let entry = self
            .model_registry
            .find_for_role(EngineRole::Embedding)
            .map_err(|err| AppError::model(format!("model registry lookup for EngineRole::Embedding failed: {}", err.message)))?
            .ok_or_else(|| {
                AppError::model(
                    "no model currently assigned to EngineRole::Embedding -- pull an embedding-capable \
                     model in Ollama (any model whose /api/show capabilities include \"embedding\") and \
                     restart Atlas so Model Discovery can assign it, or assign one manually in Settings",
                )
            })?;
        Ok(entry.model_identifier)
    }
}

impl EmbeddingEngine for OllamaEmbeddingEngine {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// `"ollama:<model>"` using whichever model is *currently* assigned to
    /// `EngineRole::Embedding` (§22, §33): if resolution itself fails (no
    /// model assigned / registry unreachable), reports that plainly rather
    /// than fabricating a provider id -- callers of `embed`/`embed_batch`
    /// will already have gotten a hard `ModelError` in that case, so this
    /// value is only ever observed alongside a real embedding.
    fn provider_id(&self) -> String {
        match self.resolve_model() {
            Ok(model) => format!("ollama:{model}"),
            Err(_) => "ollama:unresolved".to_string(),
        }
    }

    fn embed(&self, text: &str) -> Result<Embedding, AppError> {
        let model = self.resolve_model()?;
        self.ollama.embed(&model, text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Embedding>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let model = self.resolve_model()?;
        let refs: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();
        self.ollama.embed_batch(&model, &refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::OllamaConnection;
    use crate::registry::InMemoryModelRegistry;
    use atlas_types::ids::ModelRegistryId;
    use atlas_types::model::{ModelRegistryEntry, ModelStatus};

    fn registry_with_embedding_model(model_identifier: &str) -> Arc<dyn ModelRegistryRepository> {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(ModelRegistryEntry {
                id: ModelRegistryId(0),
                model_identifier: model_identifier.to_string(),
                engine_role: EngineRole::Embedding,
                capabilities: serde_json::json!(["embedding"]),
                context_length: 4096,
                vram_requirement: None,
                status: ModelStatus::Available,
                version: "1".to_string(),
                supported_tasks: serde_json::json!([]),
                is_selected_for_role: true,
            })
            .unwrap();
        Arc::new(registry)
    }

    #[test]
    fn embed_fails_with_a_model_error_when_no_embedding_model_is_assigned() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let engine = OllamaEmbeddingEngine::new(ollama, registry, 1024);

        let err = engine.embed("hello").unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn embed_fails_with_a_model_error_rather_than_a_fake_vector_when_ollama_is_unreachable() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry = registry_with_embedding_model("test-embed-model:latest");
        let engine = OllamaEmbeddingEngine::new(ollama, registry, 1024);

        let err = engine.embed("hello").unwrap_err();
        assert_eq!(err.category, atlas_utils::ErrorCategory::ModelError);
    }

    #[test]
    fn embed_batch_of_empty_input_short_circuits_without_a_model_lookup() {
        // No embedding model assigned; if this looked up a model first it
        // would return a ModelError instead of an empty Ok.
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let engine = OllamaEmbeddingEngine::new(ollama, registry, 1024);

        assert_eq!(engine.embed_batch(&[]).unwrap(), Vec::<Embedding>::new());
    }

    #[test]
    fn dimensions_reports_the_configured_value() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let engine = OllamaEmbeddingEngine::new(ollama, registry, 4096);

        assert_eq!(engine.dimensions(), 4096);
    }
}
