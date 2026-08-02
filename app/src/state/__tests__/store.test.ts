import { describe, expect, it, beforeEach } from "vitest";

import { useAppStore } from "@/state/store";
import type { Workspace } from "@/ipc/types";

function makeWorkspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: 1,
    root_path: "/tmp/knowledge",
    display_name: "Test Workspace",
    status: "Active",
    created_at: "2026-01-01T00:00:00Z",
    last_indexed_at: null,
    ...overrides,
  };
}

describe("useAppStore", () => {
  beforeEach(() => {
    useAppStore.setState({
      workspaces: [],
      tabs: [],
      activeTabId: null,
      activeWorkspaceId: null,
      currentView: "dashboard",
      toasts: [],
    });
  });

  it("upserts a new workspace", () => {
    const workspace = makeWorkspace();
    useAppStore.getState().upsertWorkspace(workspace);
    expect(useAppStore.getState().workspaces).toEqual([workspace]);
  });

  it("updates an existing workspace in place instead of duplicating it", () => {
    const workspace = makeWorkspace();
    useAppStore.getState().upsertWorkspace(workspace);
    useAppStore.getState().upsertWorkspace({ ...workspace, display_name: "Renamed" });
    expect(useAppStore.getState().workspaces).toHaveLength(1);
    expect(useAppStore.getState().workspaces[0].display_name).toBe("Renamed");
  });

  it("opening a tab sets it active and switches the current view", () => {
    useAppStore.getState().openTab({ id: "workspace:1", title: "T", view: "workspace-detail", workspaceId: 1 });
    const state = useAppStore.getState();
    expect(state.activeTabId).toBe("workspace:1");
    expect(state.currentView).toBe("workspace-detail");
    expect(state.activeWorkspaceId).toBe(1);
  });

  it("closing the active tab falls back to dashboard", () => {
    useAppStore.getState().openTab({ id: "workspace:1", title: "T", view: "workspace-detail", workspaceId: 1 });
    useAppStore.getState().closeTab("workspace:1");
    const state = useAppStore.getState();
    expect(state.tabs).toHaveLength(0);
    expect(state.currentView).toBe("dashboard");
  });

  it("removing a workspace also closes its tab", () => {
    useAppStore.getState().openTab({ id: "workspace:1", title: "T", view: "workspace-detail", workspaceId: 1 });
    useAppStore.getState().removeWorkspace(1);
    expect(useAppStore.getState().tabs).toHaveLength(0);
    expect(useAppStore.getState().workspaces).toHaveLength(0);
  });

  it("pushes and dismisses toasts", () => {
    useAppStore.getState().pushToast({ kind: "success", message: "Done" });
    const id = useAppStore.getState().toasts[0].id;
    expect(useAppStore.getState().toasts).toHaveLength(1);
    useAppStore.getState().dismissToast(id);
    expect(useAppStore.getState().toasts).toHaveLength(0);
  });
});
