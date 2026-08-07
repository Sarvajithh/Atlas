import { ipcInvoke } from "@/ipc/client";
import type { LearningProgress, WeakTopic } from "@/ipc/types";

/** `memory.*` namespace (§43.1). Mirrors backend `memory_get_weaknesses`. */
export function memoryGetWeaknesses(conceptNodeId: number): Promise<LearningProgress | null> {
  return ipcInvoke<LearningProgress | null>("memory_get_weaknesses", {
    conceptNodeId,
  });
}

/**
 * The computed weak-topic aggregate for a workspace (§ Learning subsystem
 * weak-topic detection) -- real correctness counts per topic tag from
 * every recorded quiz attempt, ordered weakest (lowest accuracy) first.
 * Backs `MemoryAnalyticsView`'s weak-topic chart.
 */
export function memoryListWeakTopics(workspaceId: number): Promise<WeakTopic[]> {
  return ipcInvoke<WeakTopic[]>("memory_list_weak_topics", { workspaceId });
}
