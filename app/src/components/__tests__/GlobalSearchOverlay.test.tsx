import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { GlobalSearchOverlay } from "@/components/GlobalSearchOverlay";
import { useAppStore } from "@/state/store";
import type { GlobalSearchResult } from "@/ipc/types";

function makeResult(overrides: Partial<GlobalSearchResult> = {}): GlobalSearchResult {
  return {
    document_id: 1,
    workspace_id: 1,
    workspace_name: "Test Workspace",
    chunk_id: 10,
    relative_path: "notes/chapter1.md",
    snippet: "...matching text...",
    location_ref: "p.1",
    score: 0.9,
    ...overrides,
  };
}

describe("GlobalSearchOverlay", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({
      isGlobalSearchOpen: false,
      activeWorkspaceId: null,
      activeDocumentId: null,
      workspaces: [],
      tabs: [],
      activeTabId: null,
      currentView: "dashboard",
    });
  });

  it("renders nothing when closed", () => {
    render(<GlobalSearchOverlay />);
    expect(screen.queryByLabelText("Global search")).toBeNull();
  });

  it("does not call search_global until the user types a query", () => {
    useAppStore.setState({ isGlobalSearchOpen: true });
    render(<GlobalSearchOverlay />);
    expect(screen.getByLabelText("Global search")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("searches and renders real results from the IPC round-trip, no mock data baked into the component", async () => {
    useAppStore.setState({ isGlobalSearchOpen: true });
    vi.mocked(invoke).mockResolvedValueOnce([makeResult()]);
    const user = userEvent.setup();
    render(<GlobalSearchOverlay />);

    await user.type(screen.getByLabelText("Search query"), "photosynthesis");

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("search_global", expect.objectContaining({ query: "photosynthesis" })));
    await waitFor(() => expect(screen.getByText("notes/chapter1.md")).toBeTruthy());
  });

  it("navigates to the selected result's document and closes the overlay", async () => {
    useAppStore.setState({ isGlobalSearchOpen: true });
    vi.mocked(invoke).mockResolvedValueOnce([makeResult()]);
    const user = userEvent.setup();
    render(<GlobalSearchOverlay />);

    await user.type(screen.getByLabelText("Search query"), "photosynthesis");
    await waitFor(() => expect(screen.getByText("notes/chapter1.md")).toBeTruthy());
    await user.click(screen.getByText("notes/chapter1.md"));

    expect(useAppStore.getState().activeDocumentId).toBe(1);
    expect(useAppStore.getState().activeWorkspaceId).toBe(1);
    expect(useAppStore.getState().isGlobalSearchOpen).toBe(false);
  });

  it("scopes the query to the active workspace by default when one is open", async () => {
    useAppStore.setState({ isGlobalSearchOpen: true, activeWorkspaceId: 7 });
    vi.mocked(invoke).mockResolvedValueOnce([]);
    const user = userEvent.setup();
    render(<GlobalSearchOverlay />);

    await user.type(screen.getByLabelText("Search query"), "notes");

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("search_global", expect.objectContaining({ workspaceId: 7 })),
    );
  });
});
