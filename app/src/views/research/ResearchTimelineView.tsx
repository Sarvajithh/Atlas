import { useEffect, useState } from "react";

import { documentList } from "@/ipc/document";
import type { DocumentRecord, Workspace } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

/**
 * Research Mode Timeline tab (§8.2.4). Previously this was intentionally
 * unimplemented rather than fabricated: `DocumentRecord` only carried a
 * filesystem `mtime`, and sorting by that would be actively misleading
 * (a re-saved older paper would sort as "recent"). `DocumentRecord.authored_at`
 * is now a real, best-effort publication/authored date populated at parse
 * time (`atlas_indexer::dates`) -- this renders documents that have one,
 * sorted chronologically, and is explicit about the (likely large) set
 * that don't: it shows them in a separate "no known date" group instead
 * of silently omitting them or falling back to `mtime`.
 */
export function ResearchTimelineView({ workspaces }: { workspaces: Workspace[] }) {
  const [selectedWorkspaceIds, setSelectedWorkspaceIds] = useState<number[]>(workspaces.map((w) => w.id));
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (selectedWorkspaceIds.length === 0) {
      setDocuments([]);
      return;
    }
    let cancelled = false;
    setIsLoading(true);
    setError(null);
    Promise.all(selectedWorkspaceIds.map((id) => documentList(id)))
      .then((lists) => {
        if (!cancelled) setDocuments(lists.flat());
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedWorkspaceIds]);

  function toggleWorkspace(id: number) {
    setSelectedWorkspaceIds((prev) => (prev.includes(id) ? prev.filter((w) => w !== id) : [...prev, id]));
  }

  const dated = documents
    .filter((d): d is DocumentRecord & { authored_at: string } => d.authored_at !== null)
    .sort((a, b) => a.authored_at.localeCompare(b.authored_at));
  const undated = documents.filter((d) => d.authored_at === null);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-2">
        {workspaces.map((w) => (
          <button
            key={w.id}
            type="button"
            onClick={() => toggleWorkspace(w.id)}
            aria-pressed={selectedWorkspaceIds.includes(w.id)}
            className="rounded-md border px-2 py-1 text-xs aria-[pressed=true]:bg-accent"
          >
            {w.display_name}
          </button>
        ))}
      </div>

      {isLoading ? (
        <LoadingState label="Loading timeline…" />
      ) : error ? (
        <ErrorState message={error} />
      ) : documents.length === 0 ? (
        <EmptyState title="No documents" description="Select a workspace with indexed documents to see a timeline." />
      ) : (
        <div className="flex flex-col gap-4">
          {dated.length > 0 ? (
            <ol className="relative flex flex-col gap-3 border-l pl-4">
              {dated.map((doc) => (
                <li key={doc.id} className="relative">
                  <span className="absolute -left-[21px] top-1 h-2 w-2 rounded-full bg-primary" />
                  <p className="text-xs text-muted-foreground">{doc.authored_at}</p>
                  <p className="text-sm">{doc.relative_path}</p>
                </li>
              ))}
            </ol>
          ) : (
            <p className="text-sm text-muted-foreground">
              No documents in this selection have a known authored date yet.
            </p>
          )}

          {undated.length > 0 ? (
            <div className="rounded-md border p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">
                No known date ({undated.length} document{undated.length === 1 ? "" : "s"})
              </p>
              <ul className="flex flex-col gap-1">
                {undated.map((doc) => (
                  <li key={doc.id} className="text-sm text-muted-foreground">
                    {doc.relative_path}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
