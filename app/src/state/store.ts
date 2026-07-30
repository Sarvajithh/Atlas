import { create } from "zustand";

/**
 * Cross-cutting app state (§13): active workspace, active document,
 * assistant panel state, indexing status. A single lightweight global
 * store -- no Redux, no additional state library, without amendment.
 *
 * This store is a cache of backend truth, not a second source of truth
 * (§13). Population from IPC calls is deferred to the UI implementation
 * milestone.
 */
export interface AppState {
  activeWorkspaceId: number | null;
  activeDocumentId: number | null;
  isAssistantPanelOpen: boolean;
  setActiveWorkspaceId: (id: number | null) => void;
  setActiveDocumentId: (id: number | null) => void;
  setAssistantPanelOpen: (open: boolean) => void;
}

export const useAppStore = create<AppState>((set) => ({
  activeWorkspaceId: null,
  activeDocumentId: null,
  isAssistantPanelOpen: true,
  setActiveWorkspaceId: (id) => set({ activeWorkspaceId: id }),
  setActiveDocumentId: (id) => set({ activeDocumentId: id }),
  setAssistantPanelOpen: (open) => set({ isAssistantPanelOpen: open }),
}));
