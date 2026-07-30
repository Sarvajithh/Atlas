import { ipcInvoke } from "@/ipc/client";
import type { ConceptNode } from "@/ipc/types";

/** `graph.*` namespace (§43.1). Mirrors backend `graph_get`. */
export function graphGet(workspaceId: number): Promise<ConceptNode[]> {
  return ipcInvoke<ConceptNode[]>("graph_get", { workspaceId });
}
