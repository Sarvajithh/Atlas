import { useEffect, useMemo, useState } from "react";

import { searchGlobal } from "@/ipc/search";
import type { GlobalSearchResult } from "@/ipc/types";
import { useAppStore } from "@/state/store";

/**
 * Global Search overlay (§9, §8.1's Title Bar "search"). Hybrid
 * keyword+vector search, reranked, over either the active workspace or
 * every workspace -- every result comes from a real `search_global` IPC
 * round-trip against real indexed data, no mock/sample results (§9).
 *
 * Debounced query-as-you-type, like the rest of the app's IPC-backed
 * lists (§13): this is a thin view over backend state, not a second
 * source of truth.
 */
export function GlobalSearchOverlay() {
  const isOpen = useAppStore((s) => s.isGlobalSearchOpen);
  const setOpen = useAppStore((s) => s.setGlobalSearchOpen);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const workspaces = useAppStore((s) => s.workspaces);
  const setActiveWorkspaceId = useAppStore((s) => s.setActiveWorkspaceId);
  const setActiveDocumentId = useAppStore((s) => s.setActiveDocumentId);
  const openTab = useAppStore((s) => s.openTab);

  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<"workspace" | "all">(activeWorkspaceId !== null ? "workspace" : "all");
  const [results, setResults] = useState<GlobalSearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeWorkspaceId) ?? null,
    [workspaces, activeWorkspaceId],
  );

  // Reset transient state each time the overlay is (re)opened, so a
  // previous search doesn't linger stale behind a fresh Ctrl/Cmd+K.
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setResults([]);
      setError(null);
      setScope(activeWorkspaceId !== null ? "workspace" : "all");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const trimmed = query.trim();
    if (trimmed.length === 0) {
      setResults([]);
      setError(null);
      return;
    }
    const workspaceId = scope === "workspace" ? (activeWorkspaceId ?? undefined) : undefined;
    let cancelled = false;
    setIsSearching(true);
    const handle = window.setTimeout(() => {
      searchGlobal({ query: trimmed, workspaceId })
        .then((hits) => {
          if (cancelled) return;
          setResults(hits);
          setError(null);
        })
        .catch((err) => {
          if (cancelled) return;
          setError(err instanceof Error ? err.message : String(err));
        })
        .finally(() => {
          if (!cancelled) setIsSearching(false);
        });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [isOpen, query, scope, activeWorkspaceId]);

  useEffect(() => {
    if (!isOpen) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isOpen, setOpen]);

  if (!isOpen) return null;

  function openResult(result: GlobalSearchResult) {
    setActiveWorkspaceId(result.workspace_id);
    setActiveDocumentId(result.document_id);
    openTab({
      id: `document:${result.document_id}`,
      title: result.relative_path,
      view: "document-view",
      workspaceId: result.workspace_id,
    });
    setOpen(false);
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Global search"
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24"
      onClick={() => setOpen(false)}
    >
      <div
        className="w-full max-w-xl rounded-lg border bg-card shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b p-3">
          <input
            autoFocus
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search documents..."
            aria-label="Search query"
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
          <div className="flex items-center gap-1 text-xs">
            <button
              type="button"
              aria-pressed={scope === "workspace"}
              disabled={activeWorkspaceId === null}
              onClick={() => setScope("workspace")}
              title={
                activeWorkspaceId === null
                  ? "Open a workspace to search just that workspace"
                  : `Search only ${activeWorkspace?.display_name ?? "this workspace"}`
              }
              className="rounded px-2 py-1 hover:bg-accent aria-[pressed=true]:bg-accent disabled:opacity-40"
            >
              This workspace
            </button>
            <button
              type="button"
              aria-pressed={scope === "all"}
              onClick={() => setScope("all")}
              title="Search every workspace"
              className="rounded px-2 py-1 hover:bg-accent aria-[pressed=true]:bg-accent"
            >
              All workspaces
            </button>
          </div>
        </div>

        <div className="max-h-96 overflow-auto p-1">
          {error ? (
            <p className="p-3 text-sm text-destructive">{error}</p>
          ) : query.trim().length === 0 ? (
            <p className="p-3 text-sm text-muted-foreground">Type to search indexed documents.</p>
          ) : isSearching && results.length === 0 ? (
            <p className="p-3 text-sm text-muted-foreground">Searching…</p>
          ) : results.length === 0 ? (
            <p className="p-3 text-sm text-muted-foreground">No results for "{query.trim()}".</p>
          ) : (
            <ul>
              {results.map((result) => (
                <li key={`${result.document_id}-${result.chunk_id}`}>
                  <button
                    type="button"
                    onClick={() => openResult(result)}
                    className="flex w-full flex-col items-start gap-0.5 rounded px-3 py-2 text-left hover:bg-accent"
                  >
                    <span className="flex w-full items-center justify-between gap-2 text-sm font-medium">
                      <span className="truncate">{result.relative_path}</span>
                      {scope === "all" ? (
                        <span className="shrink-0 text-xs font-normal text-muted-foreground">
                          {result.workspace_name}
                        </span>
                      ) : null}
                    </span>
                    <span className="line-clamp-2 text-xs text-muted-foreground">{result.snippet}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
