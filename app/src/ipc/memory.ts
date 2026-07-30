import { ipcInvoke } from "@/ipc/client";
import type { LearningProgress } from "@/ipc/types";

/** `memory.*` namespace (§43.1). Mirrors backend `memory_get_weaknesses`. */
export function memoryGetWeaknesses(conceptNodeId: number): Promise<LearningProgress | null> {
  return ipcInvoke<LearningProgress | null>("memory_get_weaknesses", {
    conceptNodeId,
  });
}
