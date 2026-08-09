import { useAppStore } from "@/state/store";
import { cn } from "@/state/utils";
import { EmptyState, LoadingState } from "@/components/states/StateViews";

const STATUS_DOT: Record<string, string> = {
  Active: "bg-green-500",
  Indexing: "bg-amber-500",
  Linking: "bg-amber-500",
  Archived: "bg-muted-foreground",
  Unlinked: "bg-destructive",
};

/**
 * Sidebar (§8.1): linked-workspace switcher. A per-workspace file tree of
 * documents (§8.1's "file tree ... with indexing status badges per file")
 * is deliberately NOT implemented here: there is no `document.*` IPC
 * namespace in `app-tauri/src/commands` yet to list files inside a
 * workspace. Faking that tree from nothing would violate "real workspace
 * data" / "no mock services". This lists real linked workspaces
 * (`workspace.list`, §43.1) and lets the user switch between them.
 */
export function Sidebar() {
  const workspaces = useAppStore((s) => s.workspaces);
  const loading = useAppStore((s) => s.workspacesLoading);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const openTab = useAppStore((s) => s.openTab);

  return (
    <aside aria-label="Workspace file tree" className="flex h-full w-full flex-col border-r">
      <div className="border-b px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        Workspaces
      </div>
      <div className="flex-1 overflow-auto">
        {loading ? (
          <LoadingState label="Loading workspaces…" />
        ) : workspaces.length === 0 ? (
          <EmptyState title="No workspaces linked" description="Link a folder to get started." />
        ) : (
          <ul>
            {workspaces.map((workspace) => (
              <li key={workspace.id}>
                <button
                  type="button"
                  onClick={() =>
                    openTab({
                      id: `workspace:${workspace.id}`,
                      title: workspace.display_name,
                      view: "workspace-detail",
                      workspaceId: workspace.id,
                    })
                  }
                  className={cn(
                    "flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent",
                    activeWorkspaceId === workspace.id && "bg-accent",
                  )}
                >
                  <span
                    aria-hidden
                    className={cn("h-1.5 w-1.5 shrink-0 rounded-full", STATUS_DOT[workspace.status])}
                  />
                  <span className="truncate">{workspace.display_name}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </aside>
  );
}
