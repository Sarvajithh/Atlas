import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { ConceptGraphView } from "@/views/ConceptGraphView";

const workspaceA = {
  id: 1,
  root_path: "/tmp/ws-a",
  display_name: "Workspace A",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

describe("ConceptGraphView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [workspaceA], activeWorkspaceId: 1 });
  });

  it("shows an empty-workspaces hint when no workspace exists", () => {
    useAppStore.setState({ workspaces: [], activeWorkspaceId: null });
    render(<ConceptGraphView />);
    expect(screen.getByText("No workspaces yet")).toBeTruthy();
  });

  it("renders real concept nodes from graph.get instead of a blank pane", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { id: 1, workspace_id: 1, label: "Photosynthesis", description: "Light-driven energy conversion", created_at: "2026-01-01T00:00:00Z" },
    ]);

    render(<ConceptGraphView />);

    await waitFor(() => expect(screen.getByText("Photosynthesis")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("graph_get", { workspaceId: 1 });
  });

  it("shows an honest empty state when no concepts have been extracted yet", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    render(<ConceptGraphView />);
    await waitFor(() => expect(screen.getByText("No concepts extracted yet")).toBeTruthy());
  });
});
