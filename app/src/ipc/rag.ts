import { ipcInvoke } from "@/ipc/client";
import type { SearchResult } from "@/ipc/types";

/** `rag.*` namespace (§43.1). Mirrors backend `commands/rag.rs`. */

/** Single-workspace hybrid retrieval + context assembly (`rag.search`). */
export function ragSearch(workspaceId: number, query: string, limit?: number): Promise<SearchResult> {
  return ipcInvoke<SearchResult>("rag_search", { workspaceId, query, limit: limit ?? null });
}

/** `rag.getContext` -- currently an alias for `rag.search` (see backend doc comment). */
export function ragGetContext(workspaceId: number, query: string, limit?: number): Promise<SearchResult> {
  return ipcInvoke<SearchResult>("rag_get_context", { workspaceId, query, limit: limit ?? null });
}

/**
 * Which Research Mode task to run (§ objective "literature review
 * support, paper comparison"). Mirrors backend
 * `commands::rag::ResearchMode` 1:1.
 */
export type ResearchQueryMode = "literatureReview" | "paperComparison";

/**
 * Research Mode's cross-workspace synthesis query (`rag.researchQuery`,
 * § objective). Reuses the same `SearchResult` shape as `rag.search` --
 * `content` is the synthesized, citation-marked answer; `citations`
 * resolve `[n]` markers back to source chunks the same way single-
 * workspace citations do.
 */
export function ragResearchQuery(
  workspaceIds: number[],
  query: string,
  mode: ResearchQueryMode,
  limitPerWorkspace?: number,
): Promise<SearchResult> {
  return ipcInvoke<SearchResult>("rag_research_query", {
    workspaceIds,
    query,
    mode,
    limitPerWorkspace: limitPerWorkspace ?? null,
  });
}
