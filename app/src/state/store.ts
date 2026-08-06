import { create } from "zustand";

import type { Workspace } from "@/ipc/types";

/**
 * Top-level view routing (§8.2). No router library is introduced (§5 --
 * Tech Stack is frozen; React Router etc. would be a new frontend
 * dependency requiring an explicit amendment). View switching is plain
 * app state, consistent with "explicit over implicit" (§2.6): the shell
 * always knows exactly which single view is showing.
 *
 * "workspace-detail" is the document-first Workspace Explorer/dashboard
 * for one workspace; per this milestone's scope it lists what the real
 * `workspace_get`/`workspace_list` IPC surface can answer (§43.1). A file
 * tree of documents *within* a workspace needs a `document.*` IPC
 * namespace that does not exist yet in `app-tauri/src/commands` -- that
 * is out of scope here, not silently faked.
 *
 * "concept-graph", "research-mode", "quiz-exam", "memory-analytics", and
 * "document-view" (§8.2.3-§8.2.6, §8.2.2) route to the previously-built
 * but unmounted views. They render honestly against whatever backend
 * surface currently exists for them (which for most is still nothing --
 * see each view's own doc comment); wiring them into the shell here does
 * not imply their underlying feature logic is complete.
 */
export type AppView =
  | "dashboard"
  | "workspace-detail"
  | "settings"
  | "concept-graph"
  | "research-mode"
  | "quiz-exam"
  | "memory-analytics"
  | "document-view";

export interface Toast {
  id: string;
  kind: "info" | "success" | "error";
  message: string;
}

export interface TabState {
  id: string;
  title: string;
  view: AppView;
  workspaceId: number | null;
}

/**
 * Cross-cutting app state (§13): active workspace, active document,
 * assistant panel state, indexing status, view routing, tabs, toasts.
 * A single lightweight global store -- no Redux, no additional state
 * library.
 *
 * This store is a cache of backend truth, not a second source of truth
 * (§13): `workspaces` is populated from `workspace.list`/`workspace.get`
 * IPC results, never invented client-side.
 */
export interface AppState {
  activeWorkspaceId: number | null;
  activeDocumentId: number | null;
  isAssistantPanelOpen: boolean;
  isSplitViewOpen: boolean;
  isGlobalSearchOpen: boolean;
  currentView: AppView;
  workspaces: Workspace[];
  workspacesLoading: boolean;
  workspacesError: string | null;
  tabs: TabState[];
  activeTabId: string | null;
  toasts: Toast[];

  setActiveWorkspaceId: (id: number | null) => void;
  setActiveDocumentId: (id: number | null) => void;
  setAssistantPanelOpen: (open: boolean) => void;
  setSplitViewOpen: (open: boolean) => void;
  setGlobalSearchOpen: (open: boolean) => void;
  setCurrentView: (view: AppView) => void;
  setWorkspaces: (workspaces: Workspace[]) => void;
  setWorkspacesLoading: (loading: boolean) => void;
  setWorkspacesError: (error: string | null) => void;
  upsertWorkspace: (workspace: Workspace) => void;
  removeWorkspace: (id: number) => void;

  openTab: (tab: TabState) => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;

  pushToast: (toast: Omit<Toast, "id">) => void;
  dismissToast: (id: string) => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  activeWorkspaceId: null,
  activeDocumentId: null,
  isAssistantPanelOpen: true,
  isSplitViewOpen: false,
  isGlobalSearchOpen: false,
  currentView: "dashboard",
  workspaces: [],
  workspacesLoading: false,
  workspacesError: null,
  tabs: [],
  activeTabId: null,
  toasts: [],

  setActiveWorkspaceId: (id) => set({ activeWorkspaceId: id }),
  setActiveDocumentId: (id) => set({ activeDocumentId: id }),
  setAssistantPanelOpen: (open) => set({ isAssistantPanelOpen: open }),
  setSplitViewOpen: (open) => set({ isSplitViewOpen: open }),
  setGlobalSearchOpen: (open) => set({ isGlobalSearchOpen: open }),
  setCurrentView: (view) => set({ currentView: view }),
  setWorkspaces: (workspaces) => set({ workspaces }),
  setWorkspacesLoading: (loading) => set({ workspacesLoading: loading }),
  setWorkspacesError: (error) => set({ workspacesError: error }),
  upsertWorkspace: (workspace) =>
    set((state) => {
      const exists = state.workspaces.some((w) => w.id === workspace.id);
      return {
        workspaces: exists
          ? state.workspaces.map((w) => (w.id === workspace.id ? workspace : w))
          : [...state.workspaces, workspace],
      };
    }),
  removeWorkspace: (id) =>
    set((state) => ({
      workspaces: state.workspaces.filter((w) => w.id !== id),
      tabs: state.tabs.filter((t) => t.workspaceId !== id),
    })),

  openTab: (tab) =>
    set((state) => {
      const exists = state.tabs.some((t) => t.id === tab.id);
      return {
        tabs: exists ? state.tabs : [...state.tabs, tab],
        activeTabId: tab.id,
        currentView: tab.view,
        activeWorkspaceId: tab.workspaceId,
      };
    }),
  closeTab: (id) =>
    set((state) => {
      const remaining = state.tabs.filter((t) => t.id !== id);
      const wasActive = state.activeTabId === id;
      const nextActive = wasActive ? (remaining[remaining.length - 1]?.id ?? null) : state.activeTabId;
      return {
        tabs: remaining,
        activeTabId: nextActive,
        currentView: nextActive
          ? (remaining.find((t) => t.id === nextActive)?.view ?? "dashboard")
          : "dashboard",
      };
    }),
  setActiveTab: (id) => {
    const tab = get().tabs.find((t) => t.id === id);
    if (!tab) return;
    set({ activeTabId: id, currentView: tab.view, activeWorkspaceId: tab.workspaceId });
  },

  pushToast: (toast) =>
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id: crypto.randomUUID() }],
    })),
  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
}));
