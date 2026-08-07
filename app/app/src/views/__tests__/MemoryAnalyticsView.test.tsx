import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { MemoryAnalyticsView } from "@/views/MemoryAnalyticsView";

const WEAK_TOPIC = {
  topic: "Thermodynamics",
  correct_count: 1,
  incorrect_count: 3,
  accuracy: 0.25,
};

const PLAN = {
  id: 1,
  workspace_id: 1,
  items: [{ topic: "Thermodynamics", recommendation: "Review chapter 4", priority: 1 }],
  created_at: "2026-01-01T00:00:00Z",
};

describe("MemoryAnalyticsView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ activeWorkspaceId: 1 });
  });

  it("shows an empty state when no workspace is active", () => {
    useAppStore.setState({ activeWorkspaceId: null });
    render(<MemoryAnalyticsView />);
    expect(screen.getByLabelText("Memory and Analytics View")).toBeTruthy();
    expect(screen.getByText("No workspace selected")).toBeTruthy();
  });

  it("renders the real, computed weak-topic aggregate from memory_list_weak_topics -- no mock data", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "memory_list_weak_topics") return Promise.resolve([WEAK_TOPIC]);
      if (cmd === "assistant_list_revision_plans") return Promise.resolve([]);
      return Promise.resolve([]);
    });
    render(<MemoryAnalyticsView />);
    await waitFor(() => expect(screen.getByText("Thermodynamics")).toBeTruthy());
    expect(screen.getByText("25% correct")).toBeTruthy();
    expect(screen.getByText("1 correct · 3 incorrect")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("memory_list_weak_topics", { workspaceId: 1 });
  });

  it("renders the latest revision plan, sorted by priority", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "memory_list_weak_topics") return Promise.resolve([]);
      if (cmd === "assistant_list_revision_plans") return Promise.resolve([PLAN]);
      return Promise.resolve([]);
    });
    render(<MemoryAnalyticsView />);
    await waitFor(() => expect(screen.getByText("Review chapter 4")).toBeTruthy());
    expect(screen.getByText("Priority 1")).toBeTruthy();
  });

  it("generating a new plan calls assistant_revision_plan with only the workspace, then refreshes the list", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "memory_list_weak_topics") return Promise.resolve([WEAK_TOPIC]);
      if (cmd === "assistant_list_revision_plans") return Promise.resolve([]);
      if (cmd === "assistant_revision_plan") return Promise.resolve(PLAN);
      return Promise.resolve([]);
    });
    render(<MemoryAnalyticsView />);
    await waitFor(() => expect(screen.getByText("No revision plan yet")).toBeTruthy());

    await user.click(screen.getByRole("button", { name: "Generate new plan" }));

    expect(invoke).toHaveBeenCalledWith("assistant_revision_plan", { request: { workspace_id: 1 } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("assistant_list_revision_plans", { workspaceId: 1 }));
  });
});
