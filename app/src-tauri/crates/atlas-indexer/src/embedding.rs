//! Embedding Engine interface (§14.1, §37). Generates vector embeddings for
//! chunks/queries (§18). Concrete backend is resolved through the Model
//! Registry (owned by atlas-models) and injected here, mirroring the OCR
//! Engine pattern in this same crate (§17, `ocr.rs`).

use atlas_utils::AppError;

/// One embedding vector, produced for either a chunk of text or a query.
pub type Embedding = Vec<f32>;

pub trait EmbeddingEngine: Send + Sync {
    /// Dimensionality of every vector this engine produces. Fixed per
    /// engine instance so callers (e.g. the vector store, §5) can size
    /// storage up front.
    fn dimensions(&self) -> usize;

    fn embed(&self, text: &str) -> Result<Embedding, AppError>;

    /// Batch variant; the default implementation just calls [`Self::embed`]
    /// per item, but a real model-backed engine can override this to batch
    /// requests to the underlying model for efficiency.
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Embedding>, AppError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Identifies which engine/model actually produced a vector, recorded
    /// per-chunk as `EmbeddingMetadata::embedding_provider_id` (§33). This
    /// is the cache-invalidation key §22 relies on ("source file content
    /// hash + parser/engine version tag ... if either changes, the cached
    /// artifact is stale"): if the assigned embedding model changes, this
    /// value changes with it, so a stale vector from a different model is
    /// identifiable instead of silently indistinguishable from a current
    /// one. Never hardcoded by a caller (§46.1) -- each engine reports its
    /// own identity.
    fn provider_id(&self) -> String {
        "unknown-embedding-engine".to_string()
    }
}

/// A dependency-free, deterministic `EmbeddingEngine` default (§28: Ollama
/// inference is explicitly out of scope for this milestone -- "Do NOT
/// implement ... Ollama inference"). Uses simple hashed bag-of-words
/// feature hashing rather than a learned model, so:
///
/// - it never calls out to a network or a local model runtime (§2 "no
///   network calls except to a locally running Ollama instance" -- this
///   engine makes none at all),
/// - it is fully deterministic and testable without any fixture model,
/// - it still produces vectors that are near each other for texts sharing
///   vocabulary, which is enough to exercise chunking -> embedding ->
///   vector storage -> retrieval end-to-end (§18).
///
/// Swapping in a real Ollama-backed embedding model later is exactly the
/// Dependency Inversion story the Model Registry already provides (§37):
/// register a new `EmbeddingEngine` impl, resolve it via `ModelProvider`,
/// nothing above this interface changes.
pub struct HashEmbeddingEngine {
    dimensions: usize,
}

impl HashEmbeddingEngine {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions: dimensions.max(1),
        }
    }
}

impl Default for HashEmbeddingEngine {
    fn default() -> Self {
        Self::new(128)
    }
}

impl EmbeddingEngine for HashEmbeddingEngine {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_id(&self) -> String {
        "hash-embedding-engine".to_string()
    }

    fn embed(&self, text: &str) -> Result<Embedding, AppError> {
        let mut vector = vec![0f32; self.dimensions];
        for token in text.split_whitespace().map(|w| w.to_lowercase()) {
            let hash = atlas_utils::hashing::hash_str(&token);
            // Use the first 8 hex chars (32 bits) of the token's hash to
            // pick a bucket and a signed weight (feature hashing, a
            // standard dependency-free way to turn arbitrary text into a
            // fixed-size vector).
            let bucket_seed = u32::from_str_radix(&hash[0..8], 16).unwrap_or(0);
            let sign_seed = u32::from_str_radix(&hash[8..16], 16).unwrap_or(0);
            let bucket = (bucket_seed as usize) % self.dimensions;
            let sign = if sign_seed % 2 == 0 { 1.0 } else { -1.0 };
            vector[bucket] += sign;
        }

        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }
        Ok(vector)
    }
}

/// Cosine similarity between two equal-length vectors, used both by the
/// default `HashEmbeddingEngine`'s tests and by the vector store's search
/// path (§18 "Vector search"). Returns `0.0` for mismatched lengths or a
/// zero vector rather than panicking (§45.2: no silent failure, but also
/// no reason to make a defensive shape-check a hard error here).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|v| v * v).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_is_deterministic() {
        let engine = HashEmbeddingEngine::default();
        assert_eq!(
            engine.embed("linear algebra basics").unwrap(),
            engine.embed("linear algebra basics").unwrap()
        );
    }

    #[test]
    fn embed_produces_the_configured_dimensionality() {
        let engine = HashEmbeddingEngine::new(64);
        assert_eq!(engine.embed("anything").unwrap().len(), 64);
        assert_eq!(engine.dimensions(), 64);
    }

    #[test]
    fn similar_text_is_closer_than_unrelated_text() {
        let engine = HashEmbeddingEngine::default();
        let a = engine.embed("gradient descent optimizes a loss function").unwrap();
        let b = engine
            .embed("gradient descent minimizes a loss function")
            .unwrap();
        let c = engine.embed("bananas are a good source of potassium").unwrap();

        let sim_ab = cosine_similarity(&a, &b);
        let sim_ac = cosine_similarity(&a, &c);
        assert!(sim_ab > sim_ac, "expected {sim_ab} > {sim_ac}");
    }

    #[test]
    fn embed_batch_matches_individual_embed_calls() {
        let engine = HashEmbeddingEngine::default();
        let texts = vec!["alpha".to_string(), "beta".to_string()];
        let batch = engine.embed_batch(&texts).unwrap();
        assert_eq!(batch[0], engine.embed("alpha").unwrap());
        assert_eq!(batch[1], engine.embed("beta").unwrap());
    }

    #[test]
    fn cosine_similarity_of_identical_vectors_is_one() {
        let engine = HashEmbeddingEngine::default();
        let v = engine.embed("identical").unwrap();
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_mismatched_lengths_is_zero() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
    }
}
