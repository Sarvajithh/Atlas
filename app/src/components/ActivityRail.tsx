import { useAppStore } from "@/state/store";
import { cn } from "@/state/utils";

/**
 * Activity Rail (§8.1): far-left icon strip. Dashboard/Workspaces, Concept
 * Graph, Memory & Analytics, and Settings are all reachable here. Concept
 * Graph and Memory & Analytics route to their views, which render honestly
 * against whatever backend surface currently exists for them (mostly none
 * yet -- see each view's own doc comment); this only makes them reachable,
 * it doesn't claim their underlying feature logic is complete. A Global
 * Search rail entry is intentionally not added here: §8.1 calls for one,
 * but no unified hybrid-search IPC command or frontend surface exists at
 * all yet, so there is nothing for it to route to (out of this phase's
 * scope, see README's "Remaining Atlas v1.0 Work").
 */
const RAIL_ITEMS = [
  { id: "dashboard", label: "Workspaces", view: "dashboard" as const, enabled: true, glyph: "▤" },
  { id: "graph", label: "Concept Graph", view: "concept-graph" as const, enabled: true, glyph: "◇" },
  { id: "memory", label: "Memory & Analytics", view: "memory-analytics" as const, enabled: true, glyph: "◎" },
  { id: "models", label: "Model Dashboard", view: "model-dashboard" as const, enabled: true, glyph: "▣" },
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
