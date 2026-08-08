import { ipcInvoke } from "@/ipc/client";
import type { CitationGraphEdge, ConceptNode, ExtractionOutcome, GraphFullResponse } from "@/ipc/types";

/** `graph.*` namespace (§43.1). Mirrors backend `graph_get`. */
export function graphGet(workspaceId: number): Promise<ConceptNode[]> {
  return ipcInvoke<ConceptNode[]>("graph_get", { workspaceId });
}

/**
 * Full graph (nodes + edges) for node-link rendering, mirrors backend
 * `graph_get_full`. See `AppFacade::graph_full`'s doc comment for why
 * `graph_get` alone can't drive an actual graph diagram.
 */
export function graphGetFull(workspaceId: number): Promise<GraphFullResponse> {
  return ipcInvoke<GraphFullResponse>("graph_get_full", { workspaceId });
}

/**
 * Manually re-runs Concept Extraction over every already-indexed document
 * in a workspace, mirrors backend `graph_reextract`. Idempotent -- safe to
 * call repeatedly, never creates duplicate nodes/edges.
 */
export function graphReextract(workspaceId: number): Promise<ExtractionOutcome> {
  return ipcInvoke<ExtractionOutcome>("graph_reextract", { workspaceId });
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

