import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

// jsdom doesn't implement DOMMatrix; pdfjs-dist references it at module
// top-level (via the document viewer transitively pulled in by App ->
// WorkspaceDetail). Static imports are hoisted ahead of ordinary
// statements, so the shim has to be in place before App is imported --
// hence the dynamic import below instead of a static one. Test-environment
// shim only, not a runtime behavior change.
if (typeof (globalThis as unknown as { DOMMatrix?: unknown }).DOMMatrix === "undefined") {
  (globalThis as unknown as { DOMMatrix: unknown }).DOMMatrix = class DOMMatrix {};
}
const { App } = await import("@/App");

/**
 * Navigation tests for the shell's view routing (§8.2). These exercise the
 * exact same `currentView`-conditional in App.tsx that "settings" and
 * "workspace-detail" already used, now extended to the 5 previously
 * unrouted views -- confirming each is reachable through a real,
 * discoverable UI action (not only by directly setting store state).
 */
describe("App navigation", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    // Generic resolution for every IPC call the shell/panels make on
    // mount (workspace.list, settings.get, assistant.listSessions, ...);
    // individual tests don't care about the specific payloads.
    vi.mocked(invoke).mockResolvedValue([]);
    useAppStore.setState({
      currentView: "dashboard",
      activeWorkspaceId: null,
      workspaces: [],
      workspacesLoading: false,
      workspacesError: null,
      tabs: [],
      activeTabId: null,
      isAssistantPanelOpen: false,
    });
  });

  it("reaches Concept Graph from the Activity Rail", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Concept Graph" }));
    expect(screen.getByLabelText("Concept Graph View")).toBeTruthy();
    expect(useAppStore.getState().currentView).toBe("concept-graph");
  });

  it("reaches Memory & Analytics from the Activity Rail", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Memory & Analytics" }));
    expect(screen.getByLabelText("Memory and Analytics View")).toBeTruthy();
    expect(useAppStore.getState().currentView).toBe("memory-analytics");
  });

  it("reaches Document View, Research Mode, and Quiz/Exam Mode from the workspace mode switcher", async () => {
    const user = userEvent.setup();
    const workspace = {
      id: 1,
      root_path: "/tmp/knowledge",
      display_name: "Semester 5",
      status: "Active",
      created_at: "2026-01-01T00:00:00Z",
      last_indexed_at: null,
    };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "workspace_list") return Promise.resolve([workspace]);
      if (cmd === "workspace_indexing_status") {
        return Promise.resolve({
          queued: 0,
          running: null,
          succeeded: 0,
          failed: 0,
          total: 0,
          progress_percent: null,
          last_indexed_at: null,
        });
      }
      return Promise.resolve([]);
    });
    render(<App />);

    // Open the workspace first (the mode switcher only appears once a
    // workspace is active) -- via the real Sidebar entry point. Scoped to
    // the sidebar landmark since the same workspace name also appears as
    // a card in WorkspaceHome's main-content grid.
    const sidebar = screen.getByLabelText("Workspace file tree");
    await within(sidebar).findByRole("button", { name: /Semester 5/ });
    await user.click(within(sidebar).getByRole("button", { name: /Semester 5/ }));
    expect(useAppStore.getState().currentView).toBe("workspace-detail");

    const modeNav = screen.getByLabelText("Workspace modes");

    await user.click(within(modeNav).getByRole("button", { name: "Document View" }));
    expect(screen.getByLabelText("Document View")).toBeTruthy();
    expect(useAppStore.getState().currentView).toBe("document-view");

    await user.click(within(modeNav).getByRole("button", { name: "Research Mode" }));
    expect(screen.getByLabelText("Research Mode")).toBeTruthy();
    expect(useAppStore.getState().currentView).toBe("research-mode");

    await user.click(within(modeNav).getByRole("button", { name: "Quiz / Exam" }));
    expect(screen.getByLabelText("Quiz and Exam Mode")).toBeTruthy();
    expect(useAppStore.getState().currentView).toBe("quiz-exam");

    // And back to the document explorer.
    await user.click(within(modeNav).getByRole("button", { name: "Explorer" }));
    expect(useAppStore.getState().currentView).toBe("workspace-detail");
  });

  it("does not show the workspace mode switcher when no workspace is active", () => {
    render(<App />);
    expect(screen.queryByLabelText("Workspace modes")).toBeNull();
  });

  it("still reaches Settings and Dashboard without regressing existing navigation", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(useAppStore.getState().currentView).toBe("settings");

    await user.click(screen.getByRole("button", { name: "Workspaces" }));
    expect(useAppStore.getState().currentView).toBe("dashboard");
  });
});
