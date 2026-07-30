//! OCR Engine interface (§14.1, §17). OCR runs only on pages/files detected
//! as image-based, never assumed (§17). Concrete OCR backend is resolved
//! through the Model Registry (owned by atlas-models) and injected here.

use atlas_utils::AppError;

pub trait OcrEngine: Send + Sync {
    /// Extract text from a single rasterized page/image, returning raw text.
    fn extract_text(&self, image_bytes: &[u8]) -> Result<String, AppError>;
}
