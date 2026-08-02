import { useAppStore } from "@/state/store";

/**
 * Status Bar (§8.1): background indexing progress, current engine
 * activity, never blocks interaction. This milestone surfaces what real
 * IPC data supports: workspace status counts from `workspace.list`. Live
 * per-job progress needs the Background Job System's event stream
 * (§36/§34), which has no frontend IPC wrapper yet -- not fabricated here.
 */
export function StatusBar() {
  const workspaces = useAppStore((s) => s.workspaces);
  const indexing = workspaces.filter((w) => w.status === "Indexing" || w.status === "Linking").length;
  const active = workspaces.filter((w) => w.status === "Active").length;

  return (
    <footer
      aria-label="Status Bar"
      className="flex h-6 shrink-0 items-center justify-between border-t px-3 text-xs text-muted-foreground"
    >
      <span>
        {workspaces.length} workspace{workspaces.length === 1 ? "" : "s"} · {active} active
      </span>
      {indexing > 0 ? (
        <span role="status" aria-live="polite" className="flex items-center gap-1.5">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-500" />
          Indexing {indexing} workspace{indexing === 1 ? "" : "s"}…
        </span>
      ) : null}
    </footer>
  );
}
