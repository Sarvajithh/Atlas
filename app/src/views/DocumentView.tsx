/**
 * Document View (§8.2.2): PDF/Lecture/Reference Book reader with
 * annotations and bookmarks, Assistant Panel docked right.
 *
 * Previously a bare stub (`<section />`, no logic at all) despite the
 * real Viewer implementation (`DocumentViewer`, §44) already existing and
 * already being used elsewhere (inline inside `WorkspaceDetail`'s tabs).
 * The blank pane here wasn't a failed IPC call or an unmet precondition
 * that could fire on its own -- it was a missing *trigger*: nothing on
 * this route's own path could ever open a document, since
 * `activeDocumentId` (state/store.ts) is only ever set by global search,
 * and `DocumentViewer` needs a `DocumentTab` (state/documents.ts), which
 * only `DocumentExplorer`'s `onOpenDocument` callback produces.
 *
 * Fix: this view now owns that trigger itself -- a workspace picker, a
 * real `DocumentExplorer` (already backed by real `document.list` IPC)
 * to open a document, and `DocumentViewer` for whichever tab is active.
 * No new viewer logic was written; this composes the same real pieces
 * `WorkspaceDetail` already uses.
 */
import { useState } from "react";

import { useAppStore } from "@/state/store";
import { useDocumentStore } from "@/state/documents";
import { DocumentExplorer } from "@/components/document/DocumentExplorer";
import { DocumentViewer } from "@/components/document/DocumentViewer";
import { EmptyState } from "@/components/states/StateViews";

export function DocumentView() {
  const workspaces = useAppStore((s) => s.workspaces);
  const storeActiveWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const [workspaceId, setWorkspaceId] = useState<number | null>(
    storeActiveWorkspaceId ?? workspaces[0]?.id ?? null,
  );

  const openTabs = useDocumentStore((s) => s.openTabs);
  const activeTabId = useDocumentStore((s) => s.activeTabId);
  const openDocument = useDocumentStore((s) => s.openDocument);

  const activeTab = openTabs.find((t) => t.tabId === activeTabId) ?? null;

  if (workspaces.length === 0) {
    return (
      <section aria-label="Document View" className="flex h-full flex-col p-4">
        <EmptyState title="No workspaces yet" description="Link a workspace to start reading documents." />
      </section>
    );
  }

  return (
    <section aria-label="Document View" className="flex h-full flex-1 overflow-hidden">
      <aside className="w-72 shrink-0 border-r">
        <div className="border-b p-2">
          <label htmlFor="document-view-workspace" className="text-xs text-muted-foreground">
            Workspace
          </label>
          <select
            id="document-view-workspace"
            value={workspaceId ?? ""}
            onChange={(e) => setWorkspaceId(Number(e.target.value))}
            className="mt-1 w-full rounded-md border bg-background px-2 py-1 text-sm"
          >
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.display_name}
              </option>
            ))}
          </select>
        </div>
        {workspaceId !== null ? (
          <DocumentExplorer
            workspaceId={workspaceId}
            onOpenDocument={(doc) => openDocument(workspaceId, doc)}
          />
        ) : null}
      </aside>

      <div className="flex flex-1 flex-col overflow-hidden">
        {activeTab ? (
          <DocumentViewer key={activeTab.tabId} tab={activeTab} />
        ) : (
          <EmptyState
            title="No document open"
            description="Choose a file from the explorer on the left to open it here."
          />
        )}
      </div>
    </section>
  );
}
