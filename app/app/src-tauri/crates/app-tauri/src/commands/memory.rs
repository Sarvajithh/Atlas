//! `memory.*` namespace (§43.1): memory.getWeaknesses, memory.recordAttempt.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::ids::{ConceptNodeId, WorkspaceId};
use atlas_types::memory::{LearningProgress, WeakTopic};
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

/// The computed weak-topic aggregate for a workspace (§ Learning subsystem
/// weak-topic detection) -- real, incrementally-computed correctness
/// counts per topic tag from every recorded quiz attempt, ordered weakest
/// first. Backs `MemoryAnalyticsView`'s weak-topic chart.
#[tauri::command]
pub fn memory_list_weak_topics(facade: State<'_, AppFacade>, workspace_id: i64) -> Result<Vec<WeakTopic>, AppError> {
    facade.list_weak_topics(WorkspaceId(workspace_id))
}
