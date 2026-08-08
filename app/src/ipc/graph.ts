import { ipcInvoke } from "@/ipc/client";
import type { CitationGraphEdge, ConceptNode } from "@/ipc/types";

/** `graph.*` namespace (§43.1). Mirrors backend `graph_get`. */
export function graphGet(workspaceId: number): Promise<ConceptNode[]> {
  return ipcInvoke<ConceptNode[]>("graph_get", { workspaceId });
}

/**
 * Research Mode's Citation Graph (§ objective "citation graph /
 * cross-document linking"), mirrors backend `graph_citation_graph`. Every
 * edge returned spans more than one document's real recorded provenance
 * -- an empty `workspaceIds` returns an empty result, not "all
 * workspaces", so callers must pass the actual selection.
 */
export function graphCitationGraph(workspaceIds: number[]): Promise<CitationGraphEdge[]> {
  return ipcInvoke<CitationGraphEdge[]>("graph_citation_graph", { workspaceIds });
}

