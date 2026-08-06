import { ipcInvoke } from "@/ipc/client";
import type { GlobalSearchResult } from "@/ipc/types";

/**
 * `search.*` namespace (§9 Global Search, §43.1). Thin typed wrapper over
 * the backend `search_global` Tauri command
 * (`app-tauri/src/commands/search.rs`), which forwards straight to
 * `AppFacade::search_global` -- hybrid keyword+vector retrieval, reranked,
 * scoped to one workspace or all of them.
 */

export interface SearchGlobalArgs {
  query: string;
  /** Omit (or pass `null`) for "All workspaces" scope, per §9. */
  workspaceId?: number | null;
  /** Omit to use the backend's configured default (`search.default_limit`, §9). */
  limit?: number;
}

export function searchGlobal(args: SearchGlobalArgs): Promise<GlobalSearchResult[]> {
  return ipcInvoke<GlobalSearchResult[]>("search_global", {
    query: args.query,
    workspaceId: args.workspaceId ?? null,
    limit: args.limit ?? null,
  });
}
