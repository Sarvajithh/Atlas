import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { QuizExamMode } from "@/views/QuizExamMode";

const workspaceA = {
  id: 1,
  root_path: "/tmp/ws-a",
  display_name: "Workspace A",
  status: "Active" as const,
  created_at: "2026-01-01T00:00:00Z",
  last_indexed_at: null,
};

describe("QuizExamMode", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ workspaces: [workspaceA], activeWorkspaceId: 1 });
  });

  it("shows an empty-workspaces hint when no workspace exists", () => {
    useAppStore.setState({ workspaces: [], activeWorkspaceId: null });
    render(<QuizExamMode />);
    expect(screen.getByText("No workspaces yet")).toBeTruthy();
  });

  it("generates a real quiz via assistant.quiz and renders its questions, not a mock", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce({
      topic: "cell division",
      questions: [
        {
          question: "What is mitosis?",
          options: ["Cell division", "Photosynthesis", "Respiration", "Osmosis"],
          correct_index: 0,
          explanation: "Mitosis is nuclear division producing two identical daughter cells.",
        },
      ],
      citations: [{ document_id: 1, chunk_id: 1, location_ref: "p1", snippet: "mitosis is..." }],
    });

    render(<QuizExamMode />);

    await user.type(screen.getByLabelText("Topic"), "cell division");
    await user.click(screen.getByRole("button", { name: "Generate" }));

    await waitFor(() => expect(screen.getByText(/What is mitosis\?/)).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("assistant_quiz", {
      request: { workspace_id: 1, topic: "cell division", question_count: null },
    });
  });

  it("submits selected answers for grading and shows the score", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValueOnce({
      topic: "cell division",
      questions: [
        {
          question: "What is mitosis?",
          options: ["Cell division", "Photosynthesis"],
          correct_index: 0,
          explanation: "Mitosis is nuclear division producing two identical daughter cells.",
        },
      ],
      citations: [],
    });
    vi.mocked(invoke).mockResolvedValueOnce({
      correct_count: 1,
      total_count: 1,
      score: 1,
      results: [{ question_index: 0, selected_index: 0, correct_index: 0, correct: true }],
      matched_concept_node_id: 42,
    });

    render(<QuizExamMode />);

    await user.type(screen.getByLabelText("Topic"), "cell division");
    await user.click(screen.getByRole("button", { name: "Generate" }));
    await waitFor(() => expect(screen.getByText(/What is mitosis\?/)).toBeTruthy());

    await user.click(screen.getByText("Cell division"));
    await user.click(screen.getByRole("button", { name: "Submit answers" }));

    await waitFor(() => expect(screen.getByText(/Score: 1 \/ 1/)).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("assistant_quiz_submit", {
      request: { workspace_id: 1, topic: "cell division", questions: expect.any(Array), answers: [0] },
    });
  });
});
