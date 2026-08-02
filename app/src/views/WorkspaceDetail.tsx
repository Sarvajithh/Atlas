import { useState } from "react";

import { workspaceArchive, workspaceRename, workspaceRestore, workspaceUnlink } from "@/ipc/workspace";
import { useAppStore } from "@/state/store";
import { EmptyState } from "@/components/states/StateViews";

/**
 * Workspace Manager + Workspace Explorer (§8.2.1/§8.2). Lifecycle actions
 * (rename/archive/restore/unlink) are wired to the real backend commands
 * (§43.1, §6.1). The document/folder tree inside a workspace is
 * intentionally NOT rendered here: no `document.*` IPC command exists yet
 * to list files under a workspace, so there is nothing real to show
 * without inventing data -- an explicit empty state explains this instead
 * of a fake tree.
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

  if (!workspace) {
    return <EmptyState title="Workspace not found" description="It may have been unlinked." />;
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
    <section aria-label={`Workspace: ${workspace.display_name}`} className="flex h-full flex-col overflow-auto p-6">
      <div className="mb-4 flex items-start justify-between">
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

      <EmptyState
        title="Document explorer not available yet"
        description="Listing files inside this workspace needs a document.* IPC command that hasn't been added to the backend yet. This is disclosed rather than shown as a fake file tree."
      />

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
