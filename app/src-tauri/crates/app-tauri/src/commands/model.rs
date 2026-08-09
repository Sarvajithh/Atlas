//! `model.*` namespace (§43.1, V1.0 Part 3 -- Model Dashboard).
//! `model_list` exposes the real `model_registry` table (role assignments,
//! status, context size, VRAM) that `atlas-core::AppFacade::model_registry()`
//! and `ModelDiscoveryService` already populate from live Ollama discovery.
//! `model_select` is the manual-selection write path: the user, and only
//! the user, assigns a model to a role from the Model Dashboard.
//! `ModelRegistryRepository::select_for_role` (atlas-models) both performs
//! the assignment (unselecting any prior selection for that role first)
//! and validates that the requested model is actually a discovered
//! candidate for that role, returning a `ModelError` otherwise.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::model::{EngineRole, ModelRegistryEntry};
use atlas_utils::AppError;

#[tauri::command]
pub fn model_list(facade: State<'_, AppFacade>) -> Result<Vec<ModelRegistryEntry>, AppError> {
    facade.model_registry().list()
}

/// Manually select `model_identifier` for `role`. This is the only way a
/// role's active model ever changes -- there is no automatic selection or
/// fallback anywhere in the runtime (§ Model Dashboard requirement).
/// Errors with a `ModelError` (surfaced to the UI as a clear message) if
/// `model_identifier` was never discovered as compatible with `role`.
#[tauri::command]
pub fn model_select(
    facade: State<'_, AppFacade>,
    role: EngineRole,
    model_identifier: String,
) -> Result<ModelRegistryEntry, AppError> {
    facade.model_registry().select_for_role(role, &model_identifier)
}
