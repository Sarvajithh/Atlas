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

  const openDocument = useDocumentStore((s) => s.openDocument);
  const openTabs = useDocumentStore((s) => s.openTabs);
  const activeDocTabId = useDocumentStore((s) => s.activeTabId);

  if (!workspace) {
    return <EmptyState title="Workspace not found" description="It may have been unlinked." />;
  }

  const activeDocTab = openTabs.find((t) => t.tabId === activeDocTabId);

  function handleOpenDocument(doc: DocumentRecord) {
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

        <div className="flex shrink-0 gap-2">
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
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <aside aria-label="Document Explorer" className="w-64 shrink-0 overflow-hidden border-r">
          <DocumentExplorer workspaceId={workspaceId} onOpenDocument={handleOpenDocument} />
        </aside>

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
