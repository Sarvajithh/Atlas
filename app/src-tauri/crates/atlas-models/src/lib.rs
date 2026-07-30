//! atlas-models
//!
//! The Engines Module (§14.1): Model Registry (§37), Model Scheduler (§15),
//! Resource Manager (§38), Context Builder (§39), and Prompt Builder (§40).
//! Engines are referred to by role name everywhere in code, UI copy, and
//! docs -- never by underlying model name (§14.1, §27). Engine-to-model
//! assignment is data in the Model Registry, editable from Settings (§23),
//! never a compiled-in mapping (§37.2).

pub mod context_builder;
pub mod engine;
pub mod prompt_builder;
pub mod registry;
pub mod resource_manager;
pub mod scheduler;

pub use context_builder::ContextBuilder;
pub use engine::Engine;
pub use prompt_builder::PromptBuilder;
pub use registry::{InMemoryModelRegistry, ModelProvider, ModelRegistryRepository};
pub use resource_manager::ResourceManager;
pub use scheduler::ModelScheduler;
