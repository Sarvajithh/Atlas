import { useState } from "react";

import { workspaceArchive, workspaceRename, workspaceRestore, workspaceUnlink } from "@/ipc/workspace";
import type { DocumentRecord } from "@/ipc/types";
import { useAppStore } from "@/state/store";
import { useDocumentStore } from "@/state/documents";
import { EmptyState } from "@/components/states/StateViews";
import { DocumentExplorer } from "@/components/document/DocumentExplorer";
import { DocumentTabs } from "@/components/document/DocumentTabs";
import { DocumentViewer } from "@/components/document/DocumentViewer";
import { IndexingStatusBar } from "@/components/workspace/IndexingStatusBar";
import { ResizablePanel } from "@/components/layout/ResizablePanel";

/**
 * Workspace Manager + Workspace Explorer + Document Experience (§8.2.1/
 * §8.2). Lifecycle actions (rename/archive/restore/unlink) use the
 * existing real backend commands (§43.1, §6.1). The document/folder tree
 * and viewers use the new `document.*`/`bookmark.*` IPC (thin passthroughs
 * to the pre-existing DocumentRepository/BookmarkRepository -- see
 * `app-tauri/src/commands/document.rs`/`bookmark.rs`).
 */
export function WorkspaceDetail({ workspaceId }: { workspaceId: number }) {
  const workspace = useAppStore((s) => s.workspaces.find((w) => w.id === workspaceId));
  const upsertWorkspace = useAppStore((s) => s.upsertWorkspace);
  const removeWorkspace = useAppStore((s) => s.removeWorkspace);
  const closeTab = useAppStore((s) => s.closeTab);
  const pushToast = useAppStore((s) => s.pushToast);

  const [isRenaming, setIsRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState(workspace?.display_name ?? "");
  const [busy, setBusy] = useState(false);
  const [confirmUnlink, setConfirmUnlink] = useState(false);
  const [explorerOpen, setExplorerOpen] = useState(true);
  // The workspace header (rename/archive/unlink, path, indexing status) is
  // pure chrome once you're actually reading a document -- it used to sit
  // permanently above the reader at full height, eating vertical space
  // document content needed. It now auto-compacts into a slim breadcrumb
  // bar the moment a document tab is open, and can still be expanded back
  // (e.g. to rebuild the index) via the chevron.
  const [headerExpanded, setHeaderExpanded] = useState(true);

  const openDocument = useDocumentStore((s) => s.openDocument);
  const openTabs = useDocumentStore((s) => s.openTabs);
  const activeDocTabId = useDocumentStore((s) => s.activeTabId);

  if (!workspace) {
    return <EmptyState title="Workspace not found" description="It may have been unlinked." />;
  }

  const activeDocTab = openTabs.find((t) => t.tabId === activeDocTabId);

  function handleOpenDocument(doc: DocumentRecord) {
    // Auto-compact the header the moment a document is opened, so reading
    // starts with maximum vertical space; the chevron can still expand it
    // back (e.g. to rename/archive/rebuild the index) at any time.
    setHeaderExpanded(false);
    openDocument(workspaceId, doc);
  }

  async function runAction<T>(action: () => Promise<T>, onSuccess: (result: T) => void, successMsg: string) {
    setBusy(true);
    try {
      const result = await action();
      onSuccess(result);
      pushToast({ kind: "success", message: successMsg });
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label={`Workspace: ${workspace.display_name}`} className="flex h-full flex-col overflow-hidden">
      {headerExpanded ? (
        <div className="flex shrink-0 items-start justify-between border-b p-4">
          <div>
            {isRenaming ? (
              <div className="flex items-center gap-2">
                <input
                  autoFocus
                  value={nameDraft}
                  onChange={(e) => setNameDraft(e.target.value)}
                  className="rounded-md border bg-background px-2 py-1 text-lg font-semibold"
                />
                <button
                  type="button"
                  disabled={busy || nameDraft.trim().length === 0}
                  onClick={() =>
                    runAction(
                      () => workspaceRename(workspace.id, nameDraft.trim()),
                      (w) => {
                        upsertWorkspace(w);
                        setIsRenaming(false);
                      },
                      "Workspace renamed",
                    )
                  }
                  className="rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground"
                >
                  Save
                </button>
                <button type="button" onClick={() => setIsRenaming(false)} className="text-xs text-muted-foreground">
                  Cancel
                </button>
              </div>
            ) : (
              <h1 className="text-xl font-semibold">{workspace.display_name}</h1>
            )}
            <p className="mt-1 text-xs text-muted-foreground">{workspace.root_path}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Status: {workspace.status}
              {workspace.last_indexed_at ? ` · Last indexed ${workspace.last_indexed_at}` : ""}
            </p>
            <div className="mt-2">
              <IndexingStatusBar workspaceId={workspaceId} />
            </div>
          </div>

          <div className="flex shrink-0 items-start gap-2">
            {!isRenaming ? (
              <button
                type="button"
                disabled={busy}
                onClick={() => setIsRenaming(true)}
                className="rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
              >
                Rename
              </button>
            ) : null}
            {workspace.status === "Archived" ? (
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  runAction(() => workspaceRestore(workspace.id), upsertWorkspace, "Workspace restored")
                }
                className="rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
              >
                Restore
              </button>
            ) : (
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  runAction(() => workspaceArchive(workspace.id), upsertWorkspace, "Workspace archived")
                }
                className="rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
              >
                Archive
              </button>
            )}
            <button
              type="button"
              disabled={busy}
              onClick={() => setConfirmUnlink(true)}
              className="rounded-md border border-destructive/50 px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10"
            >
              Unlink
            </button>
            {activeDocTab ? (
              <button
                type="button"
                onClick={() => setHeaderExpanded(false)}
                aria-label="Collapse workspace details"
                title="Collapse"
                className="rounded-md border px-2 py-1.5 text-sm hover:bg-accent"
              >
                ⌃
              </button>
            ) : null}
          </div>
        </div>
      ) : (
        <div className="flex h-9 shrink-0 items-center justify-between border-b px-3 text-sm">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate font-medium">{workspace.display_name}</span>
            <span className="text-xs text-muted-foreground">Status: {workspace.status}</span>
          </div>
          <button
            type="button"
            onClick={() => setHeaderExpanded(true)}
            aria-label="Expand workspace details"
            title="Expand (rename, archive, rebuild index)"
            className="rounded-md border px-2 py-0.5 text-xs hover:bg-accent"
          >
            ⌄ Details
          </button>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {explorerOpen ? (
          <ResizablePanel
            id="workspaceDetail.explorer"
            defaultWidth={220}
            minWidth={180}
            maxWidth={480}
            handleSide="end"
            handleAriaLabel="Resize document explorer"
          >
            <aside aria-label="Document Explorer" className="flex h-full w-full flex-col overflow-hidden border-r">
              <div className="flex shrink-0 items-center justify-between border-b px-2 py-1">
                <span className="text-xs font-medium text-muted-foreground">Files</span>
                <button
                  type="button"
                  onClick={() => setExplorerOpen(false)}
                  aria-label="Collapse file explorer"
                  title="Collapse"
                  className="rounded px-1 text-xs hover:bg-accent"
                >
                  ⟨⟨
                </button>
              </div>
              <div className="min-h-0 flex-1 overflow-hidden">
                <DocumentExplorer workspaceId={workspaceId} onOpenDocument={handleOpenDocument} />
              </div>
            </aside>
          </ResizablePanel>
        ) : (
          <button
            type="button"
            onClick={() => setExplorerOpen(true)}
            aria-label="Expand file explorer"
            title="Show files"
            className="flex w-6 shrink-0 items-center justify-center border-r text-xs text-muted-foreground hover:bg-accent"
          >
            ⟩⟩
          </button>
        )}

        <div className="flex flex-1 flex-col overflow-hidden">
          <DocumentTabs />
          {activeDocTab ? (
            <DocumentViewer key={activeDocTab.tabId} tab={activeDocTab} />
          ) : (
            <EmptyState
              title="No document open"
              description="Select a file from the Document Explorer on the left to open it."
            />
          )}
        </div>
      </div>

      {confirmUnlink ? (
        <div role="dialog" aria-modal="true" className="fixed inset-0 z-40 flex items-center justify-center bg-black/40">
          <div className="w-full max-w-sm rounded-lg border bg-card p-5 shadow-lg">
            <h2 className="mb-1 text-base font-semibold">Unlink “{workspace.display_name}”?</h2>
            <p className="mb-4 text-sm text-muted-foreground">
              This removes the workspace link and stops watching. Per §6.1, derived knowledge (AI cache, memory) is
              NOT deleted.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmUnlink(false)}
                className="rounded-md px-3 py-1.5 text-sm hover:bg-accent"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  runAction(
                    () => workspaceUnlink(workspace.id),
                    () => {
                      removeWorkspace(workspace.id);
                      closeTab(`workspace:${workspace.id}`);
                      setConfirmUnlink(false);
                    },
                    "Workspace unlinked",
                  )
                }
                className="rounded-md bg-destructive px-3 py-1.5 text-sm text-destructive-foreground"
              >
                Unlink
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
