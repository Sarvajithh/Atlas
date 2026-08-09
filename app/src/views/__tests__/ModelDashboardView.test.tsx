import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { ModelDashboardView } from "@/views/ModelDashboardView";

describe("ModelDashboardView", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("renders real registry entries grouped by role, not a blank pane", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        id: 1,
        model_identifier: "granite4.1:8b",
        engine_role: "Tutor",
        capabilities: ["TextGeneration"],
        context_length: 8192,
        vram_requirement: null,
        status: "Available",
        version: "8b",
        supported_tasks: [],
        is_selected_for_role: true,
      },
    ]);

    render(<ModelDashboardView />);

    await waitFor(() => expect(screen.getByText("granite4.1:8b")).toBeTruthy());
    expect(screen.getByText("Tutor")).toBeTruthy();
    expect(screen.getByText("Context: 8.2K tokens")).toBeTruthy();
    expect(screen.getByText("VRAM: Unknown")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("model_list", undefined);
  });

  it("shows an empty-state instead of fabricating models when discovery found none", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    render(<ModelDashboardView />);
    await waitFor(() => expect(screen.getByText("No models discovered yet")).toBeTruthy());
  });

  it("surfaces a real IPC error rather than hiding it", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("ollama unreachable"));
    render(<ModelDashboardView />);
    await waitFor(() => expect(screen.getByText("ollama unreachable")).toBeTruthy());
  });
});
