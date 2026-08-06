import { useAppStore } from "@/state/store";
import { useThemeStore } from "@/state/theme";

/** Top Navigation (§8.1): current location, and panel-level toggles. */

const CRUMBS: Partial<Record<string, string>> = {
  settings: "Settings",
  "concept-graph": "Concept Graph",
  "memory-analytics": "Memory & Analytics",
  "research-mode": "Research Mode",
  "quiz-exam": "Quiz / Exam Mode",
  "document-view": "Document View",
};

/**
 * Workspace-scoped mode switcher (§8.1: "Main Document Area: Always shows
 * the current document or the current mode (Exam Mode, Research Mode,
 * Concept Graph view)"). Only shown once a workspace is open, since these
 * modes all operate against workspace/document context. Each view renders
 * honestly against whatever backend surface currently exists for it (see
 * each view's own doc comment) -- this only makes them reachable.
 */
const MODE_ITEMS = [
  { id: "document-view", label: "Document View", view: "document-view" as const },
  { id: "research-mode", label: "Research Mode", view: "research-mode" as const },
  { id: "quiz-exam", label: "Quiz / Exam", view: "quiz-exam" as const },
];

export function TopNav() {
  const currentView = useAppStore((s) => s.currentView);
  const workspaces = useAppStore((s) => s.workspaces);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const setCurrentView = useAppStore((s) => s.setCurrentView);
  const isAssistantPanelOpen = useAppStore((s) => s.isAssistantPanelOpen);
  const setAssistantPanelOpen = useAppStore((s) => s.setAssistantPanelOpen);
  const isSplitViewOpen = useAppStore((s) => s.isSplitViewOpen);
  const setSplitViewOpen = useAppStore((s) => s.setSplitViewOpen);
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);

  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const crumb =
    currentView === "workspace-detail" && activeWorkspace
      ? activeWorkspace.display_name
      : (CRUMBS[currentView] ?? "Dashboard");

  const showModeSwitcher =
    activeWorkspaceId !== null &&
    (currentView === "workspace-detail" ||
      currentView === "document-view" ||
      currentView === "research-mode" ||
      currentView === "quiz-exam");

  return (
    <header className="flex h-10 shrink-0 items-center justify-between border-b px-3 text-sm">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span>Atlas</span>
        <span aria-hidden>/</span>
        <span className="text-foreground">{crumb}</span>
      </div>
      {showModeSwitcher ? (
        <nav aria-label="Workspace modes" className="flex items-center gap-1">
          <button
            type="button"
            aria-current={currentView === "workspace-detail"}
            onClick={() => setCurrentView("workspace-detail")}
            className="rounded px-2 py-1 text-xs hover:bg-accent aria-[current=true]:bg-accent"
          >
            Explorer
          </button>
          {MODE_ITEMS.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-current={currentView === item.view}
              onClick={() => setCurrentView(item.view)}
              className="rounded px-2 py-1 text-xs hover:bg-accent aria-[current=true]:bg-accent"
            >
              {item.label}
            </button>
          ))}
        </nav>
      ) : null}
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
