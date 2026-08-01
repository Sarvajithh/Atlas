//! OCR Engine interface (§14.1, §17). OCR runs only on pages/files detected
//! as image-based, never assumed (§17). Concrete OCR backend is resolved
//! through the Model Registry (owned by atlas-models) and injected here.

use std::io::Write;
use std::process::Command;

use atlas_utils::AppError;

pub trait OcrEngine: Send + Sync {
    /// Extract text from a single rasterized page/image, returning raw text.
    fn extract_text(&self, image_bytes: &[u8]) -> Result<String, AppError>;
}

/// Detect whether a parsed Block needs OCR (§17: "OCR runs only on
/// pages/files that need it (detected, not assumed)"). A block is
/// image-based if the Parser Layer (§36) produced it as `BlockType::Image`
/// with no text content -- the digital PDF/DOCX parsers in this crate
/// already make that determination per-page/per-run.
pub fn requires_ocr(block: &atlas_types::document::Block) -> bool {
    matches!(block.block_type, atlas_types::document::BlockType::Image) && block.text_content.trim().is_empty()
}

/// A concrete `OcrEngine` that shells out to a locally installed
/// `tesseract` binary (the de facto standard local/offline OCR engine),
/// keeping this crate free of an OCR library dependency and its native
/// build requirements (§5: local-first, no network calls). The binary
/// name/path is configuration (Governing Principle), defaulting to
/// `tesseract` on `PATH`.
///
/// If the binary is not installed, `extract_text` returns a `Recoverable`
/// `AppError::indexing` (§45.1) rather than panicking -- OCR failure for
/// one file/page must not halt indexing of the rest of a workspace (§17,
/// §21).
pub struct TesseractCliOcrEngine {
    binary_path: String,
}

impl TesseractCliOcrEngine {
    pub fn new(binary_path: impl Into<String>) -> Self {
        Self {
            binary_path: binary_path.into(),
        }
    }
}

impl Default for TesseractCliOcrEngine {
    fn default() -> Self {
        Self::new("tesseract")
    }
}

impl OcrEngine for TesseractCliOcrEngine {
    fn extract_text(&self, image_bytes: &[u8]) -> Result<String, AppError> {
        let mut tmp_in = std::env::temp_dir();
        tmp_in.push(format!(
            "atlas-ocr-in-{}-{:?}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        {
            let mut file = std::fs::File::create(&tmp_in)
                .map_err(|e| AppError::indexing(format!("failed to write OCR temp input: {e}")))?;
            file.write_all(image_bytes)
                .map_err(|e| AppError::indexing(format!("failed to write OCR temp input: {e}")))?;
        }

        let output = Command::new(&self.binary_path)
            .arg(&tmp_in)
            .arg("stdout")
            .output();

        let _ = std::fs::remove_file(&tmp_in);

        let output = output.map_err(|e| {
            AppError::indexing(format!(
                "OCR engine '{}' is not available: {e}",
                self.binary_path
            ))
        })?;

        if !output.status.success() {
            return Err(AppError::indexing(format!(
                "OCR engine '{}' exited with an error: {}",
                self.binary_path,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// A dependency-free, deterministic OCR double for tests and for
/// environments with no OCR binary installed at all -- returns a fixed
/// placeholder rather than failing, so the rest of the pipeline (chunking,
/// embedding, retrieval) remains exercisable end-to-end without requiring
/// a real OCR install (§30 testing infrastructure).
pub struct NoopOcrEngine;

impl OcrEngine for NoopOcrEngine {
    fn extract_text(&self, _image_bytes: &[u8]) -> Result<String, AppError> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_types::document::{Block, BlockType, LocationRef};

    fn block(block_type: BlockType, text: &str) -> Block {
        Block {
            block_type,
            location_ref: LocationRef {
                page_or_location: "1".to_string(),
            },
            text_content: text.to_string(),
        }
    }

    #[test]
    fn requires_ocr_true_for_empty_image_block() {
        assert!(requires_ocr(&block(BlockType::Image, "")));
    }

    #[test]
    fn requires_ocr_false_for_paragraph_block() {
        assert!(!requires_ocr(&block(BlockType::Paragraph, "")));
    }

    #[test]
    fn requires_ocr_false_for_image_block_that_already_has_text() {
        assert!(!requires_ocr(&block(BlockType::Image, "already ocr'd")));
    }

    #[test]
    fn noop_ocr_engine_returns_empty_text() {
        assert_eq!(NoopOcrEngine.extract_text(b"whatever").unwrap(), "");
    }

    #[test]
    fn tesseract_engine_reports_a_recoverable_error_when_binary_missing() {
        let engine = TesseractCliOcrEngine::new("definitely-not-a-real-binary-xyz");
        let err = engine.extract_text(b"fake image bytes").unwrap_err();
        assert_eq!(
            err.category,
            atlas_utils::ErrorCategory::Recoverable
        );
    }
}
