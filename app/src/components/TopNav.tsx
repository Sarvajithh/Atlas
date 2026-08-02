import { useAppStore } from "@/state/store";
import { useThemeStore } from "@/state/theme";

/** Top Navigation (§8.1): current location, and panel-level toggles. */
export function TopNav() {
  const currentView = useAppStore((s) => s.currentView);
  const workspaces = useAppStore((s) => s.workspaces);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const isAssistantPanelOpen = useAppStore((s) => s.isAssistantPanelOpen);
  const setAssistantPanelOpen = useAppStore((s) => s.setAssistantPanelOpen);
  const isSplitViewOpen = useAppStore((s) => s.isSplitViewOpen);
  const setSplitViewOpen = useAppStore((s) => s.setSplitViewOpen);
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);

  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const crumb =
    currentView === "settings"
      ? "Settings"
      : currentView === "workspace-detail" && activeWorkspace
        ? activeWorkspace.display_name
        : "Dashboard";

  return (
    <header className="flex h-10 shrink-0 items-center justify-between border-b px-3 text-sm">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span>Atlas</span>
        <span aria-hidden>/</span>
        <span className="text-foreground">{crumb}</span>
      </div>
      <div className="flex items-center gap-1">
        <button
          type="button"
          aria-pressed={isSplitViewOpen}
          onClick={() => setSplitViewOpen(!isSplitViewOpen)}
          title="Toggle split view"
          className="rounded px-2 py-1 text-xs hover:bg-accent"
        >
          Split
        </button>
        <button
          type="button"
          aria-pressed={isAssistantPanelOpen}
          onClick={() => setAssistantPanelOpen(!isAssistantPanelOpen)}
          title="Toggle assistant panel"
          className="rounded px-2 py-1 text-xs hover:bg-accent"
        >
          Assistant
        </button>
        <button
          type="button"
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
          title="Toggle light/dark theme"
          aria-label="Toggle theme"
          className="rounded px-2 py-1 text-xs hover:bg-accent"
        >
          {theme === "dark" ? "🌙" : "☀️"}
        </button>
      </div>
    </header>
  );
}
