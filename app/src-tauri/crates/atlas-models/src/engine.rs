//! The `Engine` trait every Engine role (§14.1) implements. This is the
//! common shape the Scheduler (§15) invokes through -- concrete engine
//! logic (Tutor, Reasoning, Retriever, etc.) is a future milestone.

use atlas_types::model::EngineRole;
use atlas_utils::AppError;

/// A fully-resolved prompt handed to an Engine (§40.1). Content assembly is
/// the Prompt Builder's responsibility, not the Engine's.
pub struct ResolvedPrompt {
    pub content: String,
    /// Base64-encoded image data, present only when the Prompt Builder
    /// assembled a Vision Engine request (§35.2 "Images (standalone):
    /// single-Block document, routed through Vision Engine"). Additive
    /// field (§46.10); text-only Engines simply never populate it.
    pub images: Option<Vec<String>>,
}

impl ResolvedPrompt {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            images: None,
        }
    }

    pub fn with_images(content: impl Into<String>, images: Vec<String>) -> Self {
        Self {
            content: content.into(),
            images: Some(images),
        }
    }
}

#[derive(Debug)]
pub struct EngineOutput {
    pub content: String,
}

pub trait Engine: Send + Sync {
    fn role(&self) -> EngineRole;
    fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError>;
}
