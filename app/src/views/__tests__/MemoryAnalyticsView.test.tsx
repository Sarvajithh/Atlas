import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { MemoryAnalyticsView } from "@/views/MemoryAnalyticsView";

const workspaceA = {
  id: 1,
  root_path: "/tmp/ws-a",
  display_name: "Workspace A",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

describe("MemoryAnalyticsView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [workspaceA], activeWorkspaceId: 1 });
  });

  it("shows an empty-workspaces hint when no workspace exists", () => {
    useAppStore.setState({ workspaces: [], activeWorkspaceId: null });
    render(<MemoryAnalyticsView />);
    expect(screen.getByText("No workspaces yet")).toBeTruthy();
  });

  it("renders real per-concept mastery data instead of a blank pane", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([
        { id: 5, workspace_id: 1, label: "Osmosis", description: null, created_at: "2026-01-01T00:00:00Z" },
      ])
      .mockResolvedValueOnce({
        concept_node_id: 5,
        mastery_score: 0.8,
        weakness_score: 0.2,
        last_reviewed_at: "2026-01-02T00:00:00Z",
        attempt_count: 3,
      });

    render(<MemoryAnalyticsView />);

    await waitFor(() => expect(screen.getByText("Osmosis")).toBeTruthy());
    expect(screen.getByText("80%")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("memory_get_weaknesses", { conceptNodeId: 5 });
  });

  it("shows 'not yet reviewed' rather than fabricating a score when no progress exists", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([
        { id: 6, workspace_id: 1, label: "Diffusion", description: null, created_at: "2026-01-01T00:00:00Z" },
      ])
      .mockResolvedValueOnce(null);

    render(<MemoryAnalyticsView />);

    await waitFor(() => expect(screen.getByText("Not yet reviewed")).toBeTruthy());
  });
});
