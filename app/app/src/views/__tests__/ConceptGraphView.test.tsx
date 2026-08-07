import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { ConceptGraphView } from "@/views/ConceptGraphView";

describe("ConceptGraphView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [], activeWorkspaceId: null });
  });

  it("shows a no-workspace-selected state when nothing is active", () => {
    render(<ConceptGraphView />);
    expect(screen.getByText("No workspace selected")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("shows an empty state when the workspace has no extracted concepts yet", async () => {
    useAppStore.setState({ activeWorkspaceId: 1, workspaces: [] });
    vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce([]);
    render(<ConceptGraphView />);
    await waitFor(() => expect(screen.getByText("No concepts yet")).toBeTruthy());
  });

  it("renders real nodes and their relations returned over IPC", async () => {
    useAppStore.setState({ activeWorkspaceId: 1, workspaces: [] });
    vi.mocked(invoke)
      .mockResolvedValueOnce([
        { id: 1, workspace_id: 1, label: "Derivatives", description: "Rate of change", created_at: "2026-01-01T00:00:00Z" },
        { id: 2, workspace_id: 1, label: "Gradient Descent", description: null, created_at: "2026-01-01T00:00:00Z" },
      ])
      .mockResolvedValueOnce([
        { id: 10, from_node_id: 1, to_node_id: 2, relation_type: "PrerequisiteOf", weight: 1 },
      ]);

    render(<ConceptGraphView />);

    await waitFor(() => expect(screen.getByText("Derivatives")).toBeTruthy());
    expect(screen.getByText("Gradient Descent")).toBeTruthy();
    expect(screen.getByText(/is a prerequisite of Gradient Descent/)).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("graph_get", { workspaceId: 1 });
    expect(invoke).toHaveBeenCalledWith("graph_get_edges", { workspaceId: 1 });
  });

  it("shows an honest error state when the IPC call fails", async () => {
    useAppStore.setState({ activeWorkspaceId: 1, workspaces: [] });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("graph query failed"));
    render(<ConceptGraphView />);
    await waitFor(() => expect(screen.getByText("graph query failed")).toBeTruthy());
  });
});
