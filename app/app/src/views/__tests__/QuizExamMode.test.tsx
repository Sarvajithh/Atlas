import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useAppStore } from "@/state/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { QuizExamMode } from "@/views/QuizExamMode";

const SAMPLE_QUIZ = {
  id: 1,
  workspace_id: 1,
  document_id: null,
  topic: "Photosynthesis",
  questions: [
    {
      question: "What pigment absorbs light?",
      options: ["Chlorophyll", "Melanin", "Keratin"],
      correct_answer: "Chlorophyll",
      source_citations: ["[1]"],
    },
  ],
  created_at: "2026-01-01T00:00:00Z",
};

describe("QuizExamMode", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useAppStore.setState({ activeWorkspaceId: 1 });
  });

  it("shows an empty state when no workspace is active", async () => {
    useAppStore.setState({ activeWorkspaceId: null });
    vi.mocked(invoke).mockResolvedValue([]);
    render(<QuizExamMode />);
    expect(screen.getByLabelText("Quiz and Exam Mode")).toBeTruthy();
    expect(screen.getByText("No workspace selected")).toBeTruthy();
  });

  it("lists previously generated quizzes from assistant_list_quizzes", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "assistant_list_quizzes") return Promise.resolve([SAMPLE_QUIZ]);
      return Promise.resolve([]);
    });
    render(<QuizExamMode />);
    await waitFor(() => expect(screen.getByText("Photosynthesis")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("assistant_list_quizzes", { workspaceId: 1 });
  });

  it("generates a quiz and renders its real, typed questions -- no mock data", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "assistant_list_quizzes") return Promise.resolve([]);
      if (cmd === "assistant_quiz") return Promise.resolve(SAMPLE_QUIZ);
      return Promise.resolve([]);
    });
    render(<QuizExamMode />);
    await waitFor(() => expect(screen.getByText("No quizzes yet")).toBeTruthy());

    await user.type(screen.getByLabelText("Topic"), "Photosynthesis");
    await user.click(screen.getByRole("button", { name: "Generate quiz" }));

    await waitFor(() =>
      expect(screen.getByText((_, el) => el?.textContent === "1. What pigment absorbs light?")).toBeTruthy(),
    );
    expect(invoke).toHaveBeenCalledWith("assistant_quiz", {
      request: { workspace_id: 1, topic: "Photosynthesis", document_id: null, question_count: 5 },
    });
    expect(screen.getByText("Chlorophyll")).toBeTruthy();
  });

  it("submits an answer per question and shows the score, using the real quiz's own topic", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "assistant_list_quizzes") return Promise.resolve([SAMPLE_QUIZ]);
      if (cmd === "assistant_get_quiz") return Promise.resolve(SAMPLE_QUIZ);
      if (cmd === "assistant_submit_quiz_answer") return Promise.resolve(undefined);
      return Promise.resolve([]);
    });
    render(<QuizExamMode />);
    await waitFor(() => expect(screen.getByText("Photosynthesis")).toBeTruthy());
    await user.click(screen.getByText("Photosynthesis"));

    await waitFor(() =>
      expect(screen.getByText((_, el) => el?.textContent === "1. What pigment absorbs light?")).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Chlorophyll" }));
    await user.click(screen.getByRole("button", { name: "Submit answers" }));

    await waitFor(() => expect(screen.getByText("Score: 1 / 1")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("assistant_submit_quiz_answer", {
      submission: { workspace_id: 1, topic: "Photosynthesis", correct: true },
    });
  });
});
