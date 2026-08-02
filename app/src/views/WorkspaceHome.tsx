import { useEffect, useState } from "react";

import { workspaceList } from "@/ipc/workspace";
import { useAppStore } from "@/state/store";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";
import { WorkspaceCreationWizard } from "@/components/WorkspaceCreationWizard";

const STATUS_LABEL: Record<string, string> = {
  Active: "Active",
  Indexing: "Indexing…",
  Linking: "Linking…",
  Archived: "Archived",
  Unlinked: "Unlinked",
};

/**
 * Workspace Home / Dashboard (§8.2.1): list of linked workspaces (active +
 * archived), quick stats. The app's default landing screen (§8.3 --
 * "never open into a blank chat box"). Data fetching via `workspace.list`
 * (§43.1) is real.
 */
export function WorkspaceHome() {
  const workspaces = useAppStore((s) => s.workspaces);
  const loading = useAppStore((s) => s.workspacesLoading);
  const error = useAppStore((s) => s.workspacesError);
  const setWorkspaces = useAppStore((s) => s.setWorkspaces);
  const setLoading = useAppStore((s) => s.setWorkspacesLoading);
  const setError = useAppStore((s) => s.setWorkspacesError);
  const openTab = useAppStore((s) => s.openTab);
  const [wizardOpen, setWizardOpen] = useState(false);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const list = await workspaceList();
      setWorkspaces(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const active = workspaces.filter((w) => w.status !== "Archived");
  const archived = workspaces.filter((w) => w.status === "Archived");

  return (
    <section aria-label="Workspace Home" className="flex h-full flex-col overflow-auto p-6">
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold">Workspaces</h1>
          <p className="text-sm text-muted-foreground">Linked folders Atlas is watching and indexing.</p>
        </div>
        <button
          type="button"
          onClick={() => setWizardOpen(true)}
          className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground"
        >
          + Link folder
        </button>
      </div>

      {loading ? (
        <LoadingState label="Loading workspaces…" />
      ) : error ? (
        <ErrorState message={error} onRetry={load} />
      ) : workspaces.length === 0 ? (
        <EmptyState
          title="No workspaces yet"
          description="Link a folder of PDFs, notes, or books to start studying with Atlas."
          action={
            <button
              type="button"
              onClick={() => setWizardOpen(true)}
              className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground"
            >
              Link your first folder
            </button>
          }
        />
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {active.map((workspace) => (
            <button
              key={workspace.id}
              type="button"
              onClick={() =>
                openTab({
                  id: `workspace:${workspace.id}`,
                  title: workspace.display_name,
                  view: "workspace-detail",
                  workspaceId: workspace.id,
                })
              }
              className="rounded-lg border p-4 text-left hover:border-primary hover:bg-accent/40"
            >
              <div className="mb-1 flex items-center justify-between">
                <span className="font-medium">{workspace.display_name}</span>
                <span className="text-xs text-muted-foreground">{STATUS_LABEL[workspace.status]}</span>
              </div>
              <p className="truncate text-xs text-muted-foreground">{workspace.root_path}</p>
              <p className="mt-2 text-xs text-muted-foreground">
                {workspace.last_indexed_at ? `Indexed ${workspace.last_indexed_at}` : "Not indexed yet"}
              </p>
            </button>
          ))}

          {archived.length > 0 ? (
            <div className="col-span-full mt-4">
              <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Archived
              </h2>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {archived.map((workspace) => (
                  <div key={workspace.id} className="rounded-lg border p-4 text-left opacity-70">
                    <span className="font-medium">{workspace.display_name}</span>
                    <p className="truncate text-xs text-muted-foreground">{workspace.root_path}</p>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      )}

      {wizardOpen ? <WorkspaceCreationWizard onClose={() => setWizardOpen(false)} /> : null}
    </section>
  );
}
