//! Model Registry shapes (§37, §33.13). Application code never references
//! model names directly; Engines resolve models through these types via the
//! `ModelProvider` interface owned by atlas-models.

use serde::{Deserialize, Serialize};

use crate::ids::ModelRegistryId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    Available,
    Loading,
    Unavailable,
    Error,
}

/// The Engine roles defined in §14.1. Application code refers to engines by
/// these names, never by underlying Ollama model name (§14.1, §27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineRole {
    Vision,
    Ocr,
    Embedding,
    Retriever,
    Reranker,
    Tutor,
    Reasoning,
    Planner,
    Memory,
    Analytics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistryEntry {
    pub id: ModelRegistryId,
    pub model_identifier: String,
    pub engine_role: EngineRole,
    pub capabilities: serde_json::Value,
    pub context_length: u32,
    pub vram_requirement: Option<u64>,
    pub status: ModelStatus,
    pub version: String,
    pub supported_tasks: serde_json::Value,
    pub is_selected_for_role: bool,
}
