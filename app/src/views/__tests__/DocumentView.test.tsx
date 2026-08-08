import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";
import { useDocumentStore } from "@/state/documents";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { DocumentView } from "@/views/DocumentView";

const workspaceA = {
  id: 1,
  root_path: "/tmp/ws-a",
  display_name: "Workspace A",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

const doc = {
  id: 1,
  workspace_id: 1,
  relative_path: "notes/chapter1.md",
  content_hash: "hash",
  file_type: "md",
  size: 100,
  mtime: "2026-01-01T00:00:00Z",
  parse_status: "Parsed" as const,
  last_indexed_hash: null,
};

describe("DocumentView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [workspaceA], activeWorkspaceId: 1 });
    useDocumentStore.setState({ openTabs: [], activeTabId: null });
  });

  it("shows an empty-workspaces hint when no workspace exists", () => {
    useAppStore.setState({ workspaces: [], activeWorkspaceId: null });
    render(<DocumentView />);
    expect(screen.getByText("No workspaces yet")).toBeTruthy();
  });

  it("shows a real explorer and a 'no document open' state until one is opened, not a blank pane", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce([doc]);
    render(<DocumentView />);

    await waitFor(() => expect(screen.getByText("notes")).toBeTruthy());
    await user.click(screen.getByText("notes"));
    expect(screen.getByText("chapter1.md")).toBeTruthy();
    expect(screen.getByText("No document open")).toBeTruthy();
  });

  it("opens a real document into the viewer when clicked in the explorer", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke)
      .mockResolvedValueOnce([doc])
      .mockResolvedValueOnce({
        relative_path: "notes/chapter1.md",
        file_type: "md",
        mime: "text/markdown",
        is_base64: false,
        content: "# Chapter 1",
      })
      .mockResolvedValueOnce([]);

    render(<DocumentView />);

    await waitFor(() => expect(screen.getByText("notes")).toBeTruthy());
    await user.click(screen.getByText("notes"));
    await user.click(screen.getByText("chapter1.md"));

    await waitFor(() => expect(screen.queryByText("No document open")).toBeNull());
  });
});
