import { useState } from "react";

import { workspaceLink } from "@/ipc/workspace";
import { useAppStore } from "@/state/store";

/**
 * Workspace Creation Wizard (§6): "Users never upload files. They link
 * folders." Calls the real `workspace_link` command, which performs the
 * initial scan and starts watching (§21) on the backend.
 *
 * The path field is a plain text input, not a native OS folder picker:
 * no `@tauri-apps/plugin-dialog` (or backend `tauri-plugin-dialog`) is
 * declared in this project's dependencies yet, and adding one is a new
 * dependency requiring an explicit §5 amendment -- not introduced here.
 */
export function WorkspaceCreationWizard({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<1 | 2>(1);
  const [rootPath, setRootPath] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const upsertWorkspace = useAppStore((s) => s.upsertWorkspace);
  const pushToast = useAppStore((s) => s.pushToast);
  const openTab = useAppStore((s) => s.openTab);

  async function handleSubmit() {
    setIsSubmitting(true);
    setError(null);
    try {
      const workspace = await workspaceLink(rootPath.trim(), displayName.trim());
      upsertWorkspace(workspace);
      pushToast({ kind: "success", message: `Linked "${workspace.display_name}"` });
      openTab({
        id: `workspace:${workspace.id}`,
        title: workspace.display_name,
        view: "workspace-detail",
        workspaceId: workspace.id,
      });
      onClose();
    } catch (err) {
      // §45.1 User Error (bad path) vs. System Error (indexing/IO fault) are
      // both surfaced honestly here rather than swallowed.
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Link a workspace folder"
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/40"
    >
      <div className="w-full max-w-md rounded-lg border bg-card p-5 shadow-lg">
        <h2 className="mb-1 text-base font-semibold">Link a folder</h2>
        <p className="mb-4 text-sm text-muted-foreground">
          Atlas watches and indexes an existing folder -- your files are never copied or uploaded (§6).
        </p>

        {step === 1 ? (
          <div className="space-y-3">
            <label className="block text-sm">
              Folder path
              <input
                autoFocus
                value={rootPath}
                onChange={(e) => setRootPath(e.target.value)}
                placeholder="/home/you/Knowledge/Semester 5"
                className="mt-1 w-full rounded-md border bg-background px-2 py-1.5 text-sm"
              />
            </label>
            <div className="flex justify-end gap-2 pt-2">
              <button type="button" onClick={onClose} className="rounded-md px-3 py-1.5 text-sm hover:bg-accent">
                Cancel
              </button>
              <button
                type="button"
                disabled={rootPath.trim().length === 0}
                onClick={() => setStep(2)}
                className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
              >
                Next
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <label className="block text-sm">
              Display name
              <input
                autoFocus
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                placeholder="Semester 5"
                className="mt-1 w-full rounded-md border bg-background px-2 py-1.5 text-sm"
              />
            </label>
            {error ? (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            ) : null}
            <div className="flex justify-end gap-2 pt-2">
              <button type="button" onClick={() => setStep(1)} className="rounded-md px-3 py-1.5 text-sm hover:bg-accent">
                Back
              </button>
              <button
                type="button"
                disabled={displayName.trim().length === 0 || isSubmitting}
                onClick={handleSubmit}
                className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
              >
                {isSubmitting ? "Linking…" : "Link folder"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
