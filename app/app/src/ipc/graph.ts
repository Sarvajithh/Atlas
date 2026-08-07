import { ipcInvoke } from "@/ipc/client";
import type { ConceptEdge, ConceptNode } from "@/ipc/types";

/** `graph.*` namespace (§43.1). Mirrors backend `graph_get`. */
export function graphGet(workspaceId: number): Promise<ConceptNode[]> {
  return ipcInvoke<ConceptNode[]>("graph_get", { workspaceId });
}

/** Mirrors backend `graph_get_edges` (Phase 5). */
export function graphGetEdges(workspaceId: number): Promise<ConceptEdge[]> {
  return ipcInvoke<ConceptEdge[]>("graph_get_edges", { workspaceId });
}

export interface ConceptDetail {
  node: ConceptNode;
  edges: ConceptEdge[];
}

/** Mirrors backend `graph_get_concept_detail` (Phase 5). */
export function graphGetConceptDetail(nodeId: number): Promise<ConceptDetail | null> {
  return ipcInvoke<ConceptDetail | null>("graph_get_concept_detail", { nodeId });
}
