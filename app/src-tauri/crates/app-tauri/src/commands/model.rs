//! `model.*` namespace (§43.1, V1.0 Part 3 -- Model Dashboard). Read-only:
//! exposes the real `model_registry` table (role assignments, status,
//! context size, VRAM) that `atlas-core::AppFacade::model_registry()` and
//! `ModelDiscoveryService` already populate from live Ollama discovery.
//! No write commands here -- Part 3 explicitly only requires the
//! dashboard to *display* this data, and assigning a model to a role from
//! the UI would need its own real design (conflict resolution when a role
//! already has a selection, etc.) that's out of scope for this pass.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::model::ModelRegistryEntry;
use atlas_utils::AppError;

#[tauri::command]
pub fn model_list(facade: State<'_, AppFacade>) -> Result<Vec<ModelRegistryEntry>, AppError> {
    facade.model_registry().list()
}
