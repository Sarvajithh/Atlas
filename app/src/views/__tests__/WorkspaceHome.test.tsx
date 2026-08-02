import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { WorkspaceHome } from "@/views/WorkspaceHome";

describe("WorkspaceHome", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [], workspacesLoading: false, workspacesError: null });
  });

  it("shows an empty state when workspace.list returns no workspaces", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    render(<WorkspaceHome />);
    await waitFor(() => expect(screen.getByText("No workspaces yet")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("workspace_list", undefined);
  });

  it("renders real workspaces returned over IPC", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        id: 1,
        root_path: "/tmp/knowledge",
        display_name: "Semester 5",
        status: "Active",
        created_at: "2026-01-01T00:00:00Z",
        last_indexed_at: null,
      },
    ]);
    render(<WorkspaceHome />);
    await waitFor(() => expect(screen.getByText("Semester 5")).toBeTruthy());
  });

  it("shows an honest error state when the IPC call fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("workspace root unreadable"));
    render(<WorkspaceHome />);
    await waitFor(() => expect(screen.getByText("workspace root unreadable")).toBeTruthy());
  });
});
