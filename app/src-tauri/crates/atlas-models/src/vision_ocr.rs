//! Vision-model-backed OCR (§14.1, §17, §37.1). `atlas_indexer::ocr`
//! already defines the `OcrEngine` trait and documents that the concrete
//! backend is "resolved through the Model Registry (owned by
//! atlas-models) and injected here" -- this is that concrete backend.
//! `discovery.rs`'s own comment previously noted OCR "continues to run
//! through the existing Tesseract pipeline ... rather than an Ollama
//! model in this milestone"; this module is that follow-up.
//!
//! Tesseract (the previous/fallback engine) is built for printed text and
//! performs badly on handwriting -- on genuinely handwritten pages it
//! commonly emits garbled, non-linguistic output rather than failing
//! cleanly, which then silently poisons chunking/embedding/retrieval
//! downstream with garbage the Tutor Engine has no way to distinguish
//! from real content. Locally-available vision-capable models (already
//! discovered into `EngineRole::Vision` by `ModelDiscoveryService`, e.g.
//! a `qwen3-vl`/`glm-ocr`-class model) read handwriting far better.

use std::sync::Arc;

use atlas_indexer::OcrEngine;
use atlas_types::model::EngineRole;
use atlas_utils::AppError;
use base64::Engine as _;

use crate::ollama::OllamaProvider;
use crate::registry::ModelRegistryRepository;

const TRANSCRIBE_PROMPT: &str = "Transcribe all text visible in this image exactly as written, including handwritten notes, printed text, and mathematical notation. Preserve line breaks and structure where reasonable. Return ONLY the transcription itself -- no commentary, no preamble, no description of the image.";

/// OCR via whichever model the Model Registry currently has selected for
/// `EngineRole::Vision` (§37: never a model name hardcoded here). Falls
/// back to `fallback` (typically `TesseractCliOcrEngine`) whenever no
/// Vision-role model is assigned or the Ollama call itself fails --
/// OCR failure for one file/page must stay Recoverable and not halt
/// indexing of the rest of a workspace (§17, §21, §45.1), the same
/// contract `TesseractCliOcrEngine` already upholds on its own.
pub struct OllamaVisionOcrEngine {
    ollama: Arc<OllamaProvider>,
    model_registry: Arc<dyn ModelRegistryRepository>,
    fallback: Arc<dyn OcrEngine>,
}

impl OllamaVisionOcrEngine {
    pub fn new(
        ollama: Arc<OllamaProvider>,
        model_registry: Arc<dyn ModelRegistryRepository>,
        fallback: Arc<dyn OcrEngine>,
    ) -> Self {
        Self {
            ollama,
            model_registry,
            fallback,
        }
    }
}

/// Whether `bytes` starts with a recognizable image file signature
/// (JPEG/PNG/GIF/BMP/WEBP) -- the formats Ollama's vision API and this
/// crate's `ImageParser` actually deal in.
fn looks_like_an_image_file(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) // JPEG
        || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) // PNG
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"BM") // BMP
        || (bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
}

impl OcrEngine for OllamaVisionOcrEngine {
    fn extract_text(&self, image_bytes: &[u8]) -> Result<String, AppError> {
        let model = match self.model_registry.find_for_role(EngineRole::Vision) {
            Ok(Some(model)) => model,
            Ok(None) => {
                atlas_utils::log_warn!(
                    "[OCR] no model assigned to EngineRole::Vision -- falling back to the non-vision OCR engine"
                );
                return self.fallback.extract_text(image_bytes);
            }
            Err(err) => {
                atlas_utils::log_warn!(
                    "[OCR] model registry lookup for EngineRole::Vision failed: {} -- falling back",
                    err.message
                );
                return self.fallback.extract_text(image_bytes);
            }
        };

        let encoded_image = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        // Defense-in-depth: this receives bytes either from
        // `PdfParser::extract_ocr_image` (whose heuristic could in
        // principle misalign) or, via `pipeline.rs`'s fallback, a whole
        // source file that might not be an image at all (e.g. a PDF
        // extraction failure falling back to the raw PDF bytes). Reject
        // anything that isn't a real image file signature before
        // spending an Ollama round-trip on it -- checks JPEG/PNG/GIF/BMP/
        // WEBP, the formats Ollama's vision API and `ImageParser` both
        // actually deal in.
        if !looks_like_an_image_file(image_bytes) {
            atlas_utils::log_warn!(
                "[OCR] extracted bytes don't match a known image file signature ({} bytes) -- falling back without calling Ollama",
                image_bytes.len()
            );
            return self.fallback.extract_text(image_bytes);
        }
        match self
            .ollama
            .generate(&model.model_identifier, TRANSCRIBE_PROMPT, Some(vec![encoded_image]), model.context_length)
        {
            Ok(text) => Ok(text.trim().to_string()),
            Err(err) => {
                atlas_utils::log_warn!(
                    "[OCR] vision model '{}' failed: {} -- falling back",
                    model.model_identifier,
                    err.message
                );
                self.fallback.extract_text(image_bytes)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::OllamaConnection;
    use crate::registry::InMemoryModelRegistry;
    use atlas_types::ids::ModelRegistryId;
    use atlas_types::model::{ModelRegistryEntry, ModelStatus};

    struct StubFallback {
        text: &'static str,
    }
    impl OcrEngine for StubFallback {
        fn extract_text(&self, _image_bytes: &[u8]) -> Result<String, AppError> {
            Ok(self.text.to_string())
        }
    }

    fn registry_with_vision_model(model_identifier: &str) -> Arc<dyn ModelRegistryRepository> {
        let registry = InMemoryModelRegistry::new();
        registry
            .upsert(ModelRegistryEntry {
                id: ModelRegistryId(0),
                model_identifier: model_identifier.to_string(),
                engine_role: EngineRole::Vision,
                capabilities: serde_json::json!(["vision"]),
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
    fn falls_back_when_no_vision_model_is_assigned() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry: Arc<dyn ModelRegistryRepository> = Arc::new(InMemoryModelRegistry::new());
        let fallback: Arc<dyn OcrEngine> = Arc::new(StubFallback { text: "fallback text" });
        let engine = OllamaVisionOcrEngine::new(ollama, registry, fallback);

        let text = engine.extract_text(b"fake image bytes").unwrap();
        assert_eq!(text, "fallback text");
    }

    #[test]
    fn falls_back_when_the_vision_call_itself_fails() {
        // Port 1 is never a reachable Ollama instance, so the HTTP call
        // inside `generate()` fails and this should still return the
        // fallback's text rather than propagating the error (§45.1).
        // Prefixed with a real JPEG signature so this exercises the
        // actual network-failure path rather than being short-circuited
        // by `looks_like_an_image_file`.
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry = registry_with_vision_model("qwen3-vl:latest");
        let fallback: Arc<dyn OcrEngine> = Arc::new(StubFallback { text: "fallback text" });
        let engine = OllamaVisionOcrEngine::new(ollama, registry, fallback);

        let mut fake_jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        fake_jpeg.extend_from_slice(b"fake jpeg body");
        let text = engine.extract_text(&fake_jpeg).unwrap();
        assert_eq!(text, "fallback text");
    }

    #[test]
    fn falls_back_without_calling_ollama_when_bytes_dont_look_like_an_image() {
        let ollama = Arc::new(OllamaProvider::new(OllamaConnection::new("127.0.0.1", 1)));
        let registry = registry_with_vision_model("qwen3-vl:latest");
        let fallback: Arc<dyn OcrEngine> = Arc::new(StubFallback { text: "fallback text" });
        let engine = OllamaVisionOcrEngine::new(ollama, registry, fallback);

        let text = engine.extract_text(b"not an image at all").unwrap();
        assert_eq!(text, "fallback text");
    }
}
