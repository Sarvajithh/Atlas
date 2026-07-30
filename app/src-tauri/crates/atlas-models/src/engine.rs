//! The `Engine` trait every Engine role (§14.1) implements. This is the
//! common shape the Scheduler (§15) invokes through -- concrete engine
//! logic (Tutor, Reasoning, Retriever, etc.) is a future milestone.

use atlas_types::model::EngineRole;
use atlas_utils::AppError;

/// A fully-resolved prompt handed to an Engine (§40.1). Content assembly is
/// the Prompt Builder's responsibility, not the Engine's.
pub struct ResolvedPrompt {
    pub content: String,
}

pub struct EngineOutput {
    pub content: String,
}

pub trait Engine: Send + Sync {
    fn role(&self) -> EngineRole;
    fn run(&self, prompt: ResolvedPrompt) -> Result<EngineOutput, AppError>;
}
