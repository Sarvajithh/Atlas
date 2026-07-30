//! `memory.*` namespace (§43.1): memory.getWeaknesses, memory.recordAttempt.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::ConceptNodeId;
use atlas_types::memory::LearningProgress;
use atlas_utils::AppError;

#[tauri::command]
pub fn memory_get_weaknesses(
    facade: State<'_, AppFacade>,
    concept_node_id: i64,
) -> Result<Option<LearningProgress>, AppError> {
    facade
        .memory_engine()
        .progress()
        .get_progress(ConceptNodeId(concept_node_id))
}
