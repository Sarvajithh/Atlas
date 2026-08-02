import { useEffect } from "react";

import { ActivityRail } from "@/components/ActivityRail";
import { Sidebar } from "@/components/Sidebar";
import { TopNav } from "@/components/TopNav";
import { Tabs } from "@/components/Tabs";
import { SplitView } from "@/components/SplitView";
import { Toaster } from "@/components/Toaster";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { StatusBar } from "@/panels/StatusBar";
import { AssistantPanel } from "@/panels/AssistantPanel";
import { WorkspaceHome } from "@/views/WorkspaceHome";
import { WorkspaceDetail } from "@/views/WorkspaceDetail";
import { SettingsView } from "@/views/SettingsView";
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
 * A full Command Palette (Ctrl+Shift+P) is out of scope for this pass --
 * it needs a registry of real, wired commands to be meaningful, and most
 * of the commands it would list (search, document actions) depend on IPC
 * surfaces (`document.*`, global search) that don't exist yet.
 */
export function App() {
  const currentView = useAppStore((s) => s.currentView);
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const isAssistantPanelOpen = useAppStore((s) => s.isAssistantPanelOpen);
  const setAssistantPanelOpen = useAppStore((s) => s.setAssistantPanelOpen);
  const isSplitViewOpen = useAppStore((s) => s.isSplitViewOpen);
  const setSplitViewOpen = useAppStore((s) => s.setSplitViewOpen);
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
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isAssistantPanelOpen, isSplitViewOpen, setAssistantPanelOpen, setSplitViewOpen]);

  let mainContent;
  if (currentView === "settings") {
    mainContent = <SettingsView />;
  } else if (currentView === "workspace-detail" && activeWorkspaceId !== null) {
    mainContent = <WorkspaceDetail workspaceId={activeWorkspaceId} />;
  } else {
    mainContent = <WorkspaceHome />;
  }

  return (
    <div className="flex h-screen w-screen flex-col">
      <TopNav />
      <div className="flex flex-1 overflow-hidden">
        <ActivityRail />
        <Sidebar />
        <div className="flex flex-1 flex-col overflow-hidden">
          <Tabs />
          <main className="flex-1 overflow-auto">
            <ErrorBoundary>
              <SplitView>{mainContent}</SplitView>
            </ErrorBoundary>
          </main>
        </div>
        {isAssistantPanelOpen ? <AssistantPanel /> : null}
      </div>
      <StatusBar />
      <Toaster />
    </div>
  );
}
