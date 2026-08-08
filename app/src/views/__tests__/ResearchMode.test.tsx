import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { ResearchMode } from "@/views/ResearchMode";

const workspaceA = {
  id: 1,
  root_path: "/tmp/ws-a",
  display_name: "Workspace A",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

const workspaceB = {
  id: 2,
  root_path: "/tmp/ws-b",
  display_name: "Workspace B",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

describe("ResearchMode", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [workspaceA, workspaceB] });
  });

  it("shows a hint to link a workspace when none exist", () => {
    useAppStore.setState({ workspaces: [] });
    render(<ResearchMode />);
    expect(screen.getByText("Link a workspace first to use Research Mode.")).toBeTruthy();
  });

  it("submits a literature review query scoped to the selected workspace and renders the real answer + citations", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce({
      content: "Sources agree that spaced repetition improves retention. [1][2]",
      citations: [
        { document_id: 10, chunk_id: 100, location_ref: "p3", snippet: "spaced repetition..." },
      ],
    });

    render(<ResearchMode />);

    await user.type(screen.getByLabelText("Research question"), "What does the literature say about retention?");
    await user.click(screen.getByRole("button", { name: "Ask" }));

    await waitFor(() =>
      expect(screen.getByText("Sources agree that spaced repetition improves retention. [1][2]")).toBeTruthy(),
    );

    expect(invoke).toHaveBeenCalledWith("rag_research_query", {
      workspaceIds: [1],
      query: "What does the literature say about retention?",
      mode: "literatureReview",
      limitPerWorkspace: null,
    });
    expect(screen.getByText(/doc #10/)).toBeTruthy();
  });

  it("switches to paper comparison mode and sends that mode over IPC", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce({ content: "Comparison result.", citations: [] });

    render(<ResearchMode />);
    await user.click(screen.getByRole("button", { name: "Paper comparison" }));
    await user.type(screen.getByLabelText("Research question"), "compare approach A and B");
    await user.click(screen.getByRole("button", { name: "Ask" }));

    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith(
      "rag_research_query",
      expect.objectContaining({ mode: "paperComparison" }),
    );
  });

  it("renders real cross-document edges in the citation graph tab", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        edge: { id: 5, from_node_id: 1, to_node_id: 2, relation_type: "RelatedTo", weight: 1 },
        from_label: "Differential Privacy",
        to_label: "K-Anonymity",
        source_document_ids: [10, 20],
      },
    ]);

    render(<ResearchMode />);
    await user.click(screen.getByRole("button", { name: "Citation graph" }));

    await waitFor(() => expect(screen.getByText("Differential Privacy")).toBeTruthy());
    expect(screen.getByText("K-Anonymity")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("graph_citation_graph", { workspaceIds: [1, 2] });
  });

  it("shows an honest empty state, not fabricated data, when the citation graph has no cross-document edges", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce([]);

    render(<ResearchMode />);
    await user.click(screen.getByRole("button", { name: "Citation graph" }));

    await waitFor(() =>
      expect(screen.getByText(/No cross-document relationships found yet/)).toBeTruthy(),
    );
  });

  it("explicitly flags Timeline as deferred rather than showing fabricated dates", async () => {
    const user = userEvent.setup();
    render(<ResearchMode />);
    await user.click(screen.getByRole("button", { name: "Timeline" }));
    expect(screen.getByText(/Timeline is deferred/)).toBeTruthy();
  });

  it("shows an honest error state when the research query IPC call fails", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("model unreachable"));

    render(<ResearchMode />);
    await user.type(screen.getByLabelText("Research question"), "anything");
    await user.click(screen.getByRole("button", { name: "Ask" }));

    await waitFor(() => expect(screen.getByText("model unreachable")).toBeTruthy());
  });
});
