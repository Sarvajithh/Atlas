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

  it("renders real concept nodes+edges from graph.getFull instead of a flat list", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      nodes: [
        { id: 1, workspace_id: 1, label: "Photosynthesis", description: "Light-driven energy conversion", created_at: "2026-01-01T00:00:00Z" },
        { id: 2, workspace_id: 1, label: "Chlorophyll", description: null, created_at: "2026-01-01T00:00:00Z" },
      ],
      edges: [{ id: 1, from_node_id: 1, to_node_id: 2, relation_type: "RelatedTo", weight: 1 }],
    });

    render(<ConceptGraphView />);

    await waitFor(() => expect(screen.getByText("Photosynthesis")).toBeTruthy());
    expect(screen.getByText("Chlorophyll")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("graph_get_full", { workspaceId: 1 });
  });

  it("shows an honest empty state when no concepts have been extracted yet", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ nodes: [], edges: [] });
    render(<ConceptGraphView />);
    await waitFor(() => expect(screen.getByText("No concepts extracted yet")).toBeTruthy());
  });

  it("clicking a node shows its relations, and re-extract calls graph.reextract", async () => {
    const user = (await import("@testing-library/user-event")).default.setup();
    vi.mocked(invoke).mockResolvedValueOnce({
      nodes: [
        { id: 1, workspace_id: 1, label: "Photosynthesis", description: null, created_at: "t" },
        { id: 2, workspace_id: 1, label: "Chlorophyll", description: null, created_at: "t" },
      ],
      edges: [{ id: 1, from_node_id: 1, to_node_id: 2, relation_type: "RelatedTo", weight: 1 }],
    });

    render(<ConceptGraphView />);
    await waitFor(() => expect(screen.getByText("Photosynthesis")).toBeTruthy());

    await user.click(screen.getByText("Photosynthesis"));
    await waitFor(() => expect(screen.getByText(/related to Chlorophyll/)).toBeTruthy());

    vi.mocked(invoke).mockResolvedValueOnce({
      nodes_created: 1,
      nodes_reused: 1,
      edges_created: 0,
      edges_skipped_existing: 1,
    });
    vi.mocked(invoke).mockResolvedValueOnce({ nodes: [], edges: [] });

    await user.click(screen.getByRole("button", { name: "Re-extract concepts" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("graph_reextract", { workspaceId: 1 }));
    await waitFor(() => expect(screen.getByText(/Re-extraction complete/)).toBeTruthy());
  });
});
