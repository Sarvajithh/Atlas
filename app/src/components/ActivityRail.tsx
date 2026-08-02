import { useAppStore } from "@/state/store";
import { cn } from "@/state/utils";

/**
 * Activity Rail (§8.1): far-left icon strip. This milestone wires the two
 * destinations backed by real functionality (Dashboard/Workspaces,
 * Settings). Search/Concept Graph/Memory rails are visible but disabled --
 * §20's graph persistence and the memory-analytics IPC surface are
 * separate, larger pieces of work not claimed as done here.
 */
const RAIL_ITEMS = [
  { id: "dashboard", label: "Workspaces", view: "dashboard" as const, enabled: true, glyph: "▤" },
  { id: "graph", label: "Concept Graph (not yet available)", view: null, enabled: false, glyph: "◇" },
  { id: "memory", label: "Memory & Analytics (not yet available)", view: null, enabled: false, glyph: "◎" },
  { id: "settings", label: "Settings", view: "settings" as const, enabled: true, glyph: "⚙" },
];

export function ActivityRail() {
  const currentView = useAppStore((s) => s.currentView);
  const setCurrentView = useAppStore((s) => s.setCurrentView);

  return (
    <nav aria-label="Activity Rail" className="flex w-12 shrink-0 flex-col items-center gap-1 border-r py-2">
      {RAIL_ITEMS.map((item) => (
        <button
          key={item.id}
          type="button"
          title={item.label}
          aria-label={item.label}
          aria-current={item.view === currentView}
          disabled={!item.enabled}
          onClick={() => item.view && setCurrentView(item.view)}
          className={cn(
            "flex h-9 w-9 items-center justify-center rounded-md text-base",
            item.enabled ? "hover:bg-accent" : "cursor-not-allowed opacity-30",
            item.view === currentView ? "bg-accent text-accent-foreground" : "text-muted-foreground",
          )}
        >
          {item.glyph}
        </button>
      ))}
    </nav>
  );
}
