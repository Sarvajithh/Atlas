import { useEffect } from "react";

import { ActivityRail } from "@/components/ActivityRail";
import { Sidebar } from "@/components/Sidebar";
import { TopNav } from "@/components/TopNav";
import { Tabs } from "@/components/Tabs";
import { SplitView } from "@/components/SplitView";
import { Toaster } from "@/components/Toaster";
import { GlobalSearchOverlay } from "@/components/GlobalSearchOverlay";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { LayoutProvider } from "@/components/layout/LayoutProvider";
import { ResizablePanel } from "@/components/layout/ResizablePanel";
import { StatusBar } from "@/panels/StatusBar";
import { AssistantPanel } from "@/panels/AssistantPanel";
import { WorkspaceHome } from "@/views/WorkspaceHome";
import { WorkspaceDetail } from "@/views/WorkspaceDetail";
import { SettingsView } from "@/views/SettingsView";
import { ConceptGraphView } from "@/views/ConceptGraphView";
import { ResearchMode } from "@/views/ResearchMode";
import { QuizExamMode } from "@/views/QuizExamMode";
import { MemoryAnalyticsView } from "@/views/MemoryAnalyticsView";
import { DocumentView } from "@/views/DocumentView";
import { ModelDashboardView } from "@/views/ModelDashboardView";
import { workspaceList } from "@/ipc/workspace";
import { useAppStore } from "@/state/store";
import { useThemeStore } from "@/state/theme";

/**
 * Shell Layout (§8.1). Document-first: the main area always shows exactly
 * one primary view (Dashboard, a workspace, or Settings); the Assistant
 * is a dockable side panel (§8.1, §2.4), never the default screen (§8.3).
 *
 * Keyboard Shortcuts (partial, scoped to what this milestone implements):
 *   Ctrl/Cmd+B        toggle sidebar-adjacent assistant panel
 *   Ctrl/Cmd+\        toggle split view
 *   Ctrl/Cmd+K        open Global Search (§9)
 * A full Command Palette (Ctrl+Shift+P) is out of scope for this pass --
 * it needs a registry of real, wired commands to be meaningful, and most
 * of the commands it would list (document actions, etc.) depend on IPC
 * surfaces (`document.*`) that don't exist yet. Global Search itself is
 * now wired (§9), separately from that palette.
 */
export function App() {
  const currentView = useAppStore((s) => s.currentView);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const isAssistantPanelOpen = useAppStore((s) => s.isAssistantPanelOpen);
  const setAssistantPanelOpen = useAppStore((s) => s.setAssistantPanelOpen);
  const isWorkspaceSidebarOpen = useAppStore((s) => s.isWorkspaceSidebarOpen);
  const setWorkspaceSidebarOpen = useAppStore((s) => s.setWorkspaceSidebarOpen);
  const isSplitViewOpen = useAppStore((s) => s.isSplitViewOpen);
  const setSplitViewOpen = useAppStore((s) => s.setSplitViewOpen);
  const setGlobalSearchOpen = useAppStore((s) => s.setGlobalSearchOpen);
  const hydrateTheme = useThemeStore((s) => s.hydrate);
  const setWorkspaces = useAppStore((s) => s.setWorkspaces);
  const setWorkspacesLoading = useAppStore((s) => s.setWorkspacesLoading);
  const setWorkspacesError = useAppStore((s) => s.setWorkspacesError);

  useEffect(() => {
    void hydrateTheme();
  }, [hydrateTheme]);

  useEffect(() => {
    setWorkspacesLoading(true);
    workspaceList()
      .then((list) => {
        setWorkspaces(list);
        setWorkspacesError(null);
      })
      .catch((err) => setWorkspacesError(err instanceof Error ? err.message : String(err)))
      .finally(() => setWorkspacesLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "b") {
        e.preventDefault();
        setAssistantPanelOpen(!isAssistantPanelOpen);
      } else if (e.key === "\\") {
        e.preventDefault();
        setSplitViewOpen(!isSplitViewOpen);
      } else if (e.key === "k") {
        e.preventDefault();
        setGlobalSearchOpen(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    isAssistantPanelOpen,
    isSplitViewOpen,
    setAssistantPanelOpen,
    setSplitViewOpen,
    setGlobalSearchOpen,
  ]);

  let mainContent;
  if (currentView === "settings") {
    mainContent = <SettingsView />;
  } else if (currentView === "workspace-detail" && activeWorkspaceId !== null) {
    mainContent = <WorkspaceDetail workspaceId={activeWorkspaceId} />;
  } else if (currentView === "concept-graph") {
    mainContent = <ConceptGraphView />;
  } else if (currentView === "research-mode") {
    mainContent = <ResearchMode />;
  } else if (currentView === "quiz-exam") {
    mainContent = <QuizExamMode />;
  } else if (currentView === "memory-analytics") {
    mainContent = <MemoryAnalyticsView />;
  } else if (currentView === "document-view") {
    mainContent = <DocumentView />;
  } else if (currentView === "model-dashboard") {
    mainContent = <ModelDashboardView />;
  } else {
    mainContent = <WorkspaceHome />;
  }

  return (
    <LayoutProvider>
      <div className="flex h-screen w-screen flex-col">
        <TopNav />
        <div className="flex flex-1 overflow-hidden">
          <ActivityRail />
          {isWorkspaceSidebarOpen ? (
            <ResizablePanel
              id="global.sidebar"
              defaultWidth={256}
              minWidth={200}
              maxWidth={480}
              handleSide="end"
              handleAriaLabel="Resize workspace sidebar"
            >
              <div className="relative flex h-full w-full">
                <Sidebar />
                <button
                  type="button"
                  onClick={() => setWorkspaceSidebarOpen(false)}
                  aria-label="Collapse workspace sidebar"
                  title="Collapse sidebar"
                  className="absolute right-1 top-1 rounded px-1 text-xs text-muted-foreground hover:bg-accent"
                >
                  ⟨⟨
                </button>
              </div>
            </ResizablePanel>
          ) : (
            <button
              type="button"
              onClick={() => setWorkspaceSidebarOpen(true)}
              aria-label="Expand workspace sidebar"
              title="Show workspaces"
              className="flex w-6 shrink-0 items-center justify-center border-r text-xs text-muted-foreground hover:bg-accent"
            >
              ⟩⟩
            </button>
          )}
          <div className="flex flex-1 flex-col overflow-hidden">
            <Tabs />
            <main className="flex-1 overflow-auto">
              <ErrorBoundary>
                <SplitView>{mainContent}</SplitView>
              </ErrorBoundary>
            </main>
          </div>
          {isAssistantPanelOpen ? (
            <ResizablePanel
              id="global.assistantPanel"
              defaultWidth={384}
              minWidth={280}
              maxWidth={720}
              handleSide="start"
              handleAriaLabel="Resize assistant panel"
            >
              <AssistantPanel />
            </ResizablePanel>
          ) : null}
        </div>
        <StatusBar />
        <Toaster />
        <GlobalSearchOverlay />
      </div>
    </LayoutProvider>
  );
}
