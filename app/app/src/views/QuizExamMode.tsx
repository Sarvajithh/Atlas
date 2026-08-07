import { useEffect, useState } from "react";

import {
  assistantGetQuiz,
  assistantListQuizzes,
  assistantQuiz,
  assistantSubmitQuizAnswer,
} from "@/ipc/assistant";
import type { Quiz } from "@/ipc/types";
import { useAppStore } from "@/state/store";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

/**
 * Quiz / Exam Mode (§8.2.5): full-focus mode, Assistant Panel hidden or
 * restricted (§8.3). Wired to the real, structured `assistant.quiz`/
 * `assistant.list_quizzes`/`assistant.submit_quiz_answer` IPC surface
 * (§ Learning subsystem structured output) -- no mock data, no free-text
 * parsing: every question rendered here is a validated
 * `QuizQuestion { question, options, correct_answer, source_citations }`
 * persisted by the backend.
 */
export function QuizExamMode() {
  const workspaceId = useAppStore((s) => s.activeWorkspaceId);
  const pushToast = useAppStore((s) => s.pushToast);

  const [pastQuizzes, setPastQuizzes] = useState<Quiz[]>([]);
  const [listLoading, setListLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);

  const [topic, setTopic] = useState("");
  const [questionCount, setQuestionCount] = useState(5);
  const [generating, setGenerating] = useState(false);
  const [generateError, setGenerateError] = useState<string | null>(null);

  const [activeQuiz, setActiveQuiz] = useState<Quiz | null>(null);
  const [answers, setAnswers] = useState<Record<number, string>>({});
  const [submitted, setSubmitted] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  async function loadPastQuizzes() {
    if (workspaceId === null) return;
    setListLoading(true);
    setListError(null);
    try {
      setPastQuizzes(await assistantListQuizzes(workspaceId));
    } catch (err) {
      setListError(err instanceof Error ? err.message : String(err));
    } finally {
      setListLoading(false);
    }
  }

  useEffect(() => {
    void loadPastQuizzes();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  function startQuiz(quiz: Quiz) {
    setActiveQuiz(quiz);
    setAnswers({});
    setSubmitted(false);
  }

  async function handleGenerate() {
    if (workspaceId === null || topic.trim().length === 0) return;
    setGenerating(true);
    setGenerateError(null);
    try {
      const quiz = await assistantQuiz(workspaceId, topic.trim(), { questionCount });
      startQuiz(quiz);
      void loadPastQuizzes();
    } catch (err) {
      setGenerateError(err instanceof Error ? err.message : String(err));
    } finally {
      setGenerating(false);
    }
  }

  async function handleResume(quizId: number) {
    try {
      const quiz = await assistantGetQuiz(quizId);
      if (quiz) startQuiz(quiz);
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }

  async function handleSubmit() {
    if (workspaceId === null || !activeQuiz) return;
    setSubmitting(true);
    try {
      // Record each answered question's outcome (§ Learning subsystem
      // weak-topic detection) -- feeds the aggregate MemoryAnalyticsView
      // reads. Sequential, not Promise.all, so a mid-batch failure doesn't
      // leave the aggregate half-updated in an unpredictable order.
      for (const [indexStr, chosen] of Object.entries(answers)) {
        const index = Number(indexStr);
        const question = activeQuiz.questions[index];
        if (!question) continue;
        await assistantSubmitQuizAnswer(workspaceId, activeQuiz.topic, chosen === question.correct_answer);
      }
      setSubmitted(true);
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    } finally {
      setSubmitting(false);
    }
  }

  if (workspaceId === null) {
    return (
      <section aria-label="Quiz and Exam Mode" className="flex h-full flex-col overflow-auto p-6">
        <EmptyState
          title="No workspace selected"
          description="Open a workspace to generate a quiz from its indexed content."
        />
      </section>
    );
  }

  const score = activeQuiz
    ? activeQuiz.questions.filter((q, i) => answers[i] === q.correct_answer).length
    : 0;

  return (
    <section aria-label="Quiz and Exam Mode" className="flex h-full flex-col overflow-auto p-6">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">Quiz Mode</h1>
        <p className="text-sm text-muted-foreground">
          Generate a quiz from this workspace's indexed content, or resume one you started earlier.
        </p>
      </div>

      {!activeQuiz ? (
        <>
          <div className="mb-6 flex flex-wrap items-end gap-3 rounded-lg border p-4">
            <div className="flex flex-col gap-1">
              <label htmlFor="quiz-topic" className="text-xs font-medium text-muted-foreground">
                Topic
              </label>
              <input
                id="quiz-topic"
                value={topic}
                onChange={(e) => setTopic(e.target.value)}
                placeholder="e.g. Photosynthesis"
                className="rounded-md border bg-background px-2 py-1.5 text-sm"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label htmlFor="quiz-count" className="text-xs font-medium text-muted-foreground">
                Questions
              </label>
              <input
                id="quiz-count"
                type="number"
                min={1}
                max={20}
                value={questionCount}
                onChange={(e) => setQuestionCount(Number(e.target.value) || 1)}
                className="w-20 rounded-md border bg-background px-2 py-1.5 text-sm"
              />
            </div>
            <button
              type="button"
              disabled={generating || topic.trim().length === 0}
              onClick={handleGenerate}
              className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
            >
              {generating ? "Generating…" : "Generate quiz"}
            </button>
            {generateError ? <p className="text-sm text-destructive">{generateError}</p> : null}
          </div>

          <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Previous quizzes
          </h2>
          {listLoading ? (
            <LoadingState label="Loading quizzes…" />
          ) : listError ? (
            <ErrorState message={listError} onRetry={loadPastQuizzes} />
          ) : pastQuizzes.length === 0 ? (
            <EmptyState title="No quizzes yet" description="Generate one above to get started." />
          ) : (
            <ul className="flex flex-col gap-2">
              {pastQuizzes.map((quiz) => (
                <li key={quiz.id}>
                  <button
                    type="button"
                    onClick={() => handleResume(quiz.id)}
                    className="w-full rounded-md border p-3 text-left text-sm hover:border-primary hover:bg-accent/40"
                  >
                    <span className="font-medium">{quiz.topic}</span>
                    <span className="ml-2 text-xs text-muted-foreground">
                      {quiz.questions.length} question{quiz.questions.length === 1 ? "" : "s"} · {quiz.created_at}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : (
        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-medium">{activeQuiz.topic}</h2>
            <button
              type="button"
              onClick={() => setActiveQuiz(null)}
              className="text-sm text-muted-foreground hover:underline"
            >
              ← Back to quiz list
            </button>
          </div>

          {submitted ? (
            <div className="rounded-lg border bg-accent/30 p-4">
              <p className="text-sm font-medium">
                Score: {score} / {activeQuiz.questions.length}
              </p>
            </div>
          ) : null}

          {activeQuiz.questions.map((question, index) => {
            const chosen = answers[index];
            return (
              <div key={index} className="rounded-lg border p-4">
                <p className="mb-3 text-sm font-medium">
                  {index + 1}. {question.question}
                </p>
                <div className="flex flex-col gap-2">
                  {question.options.map((option) => {
                    const isChosen = chosen === option;
                    const isCorrect = option === question.correct_answer;
                    let stateClass = "";
                    if (submitted) {
                      if (isCorrect) stateClass = "border-green-500 bg-green-500/10";
                      else if (isChosen) stateClass = "border-destructive bg-destructive/10";
                    } else if (isChosen) {
                      stateClass = "border-primary bg-primary/10";
                    }
                    return (
                      <button
                        key={option}
                        type="button"
                        disabled={submitted}
                        onClick={() => setAnswers((prev) => ({ ...prev, [index]: option }))}
                        className={`rounded-md border px-3 py-2 text-left text-sm disabled:cursor-default ${stateClass}`}
                      >
                        {option}
                      </button>
                    );
                  })}
                </div>
                {submitted && question.source_citations.length > 0 ? (
                  <p className="mt-2 text-xs text-muted-foreground">
                    Sources: {question.source_citations.join(", ")}
                  </p>
                ) : null}
              </div>
            );
          })}

          {!submitted ? (
            <button
              type="button"
              disabled={submitting || Object.keys(answers).length !== activeQuiz.questions.length}
              onClick={handleSubmit}
              className="self-start rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground disabled:opacity-50"
            >
              {submitting ? "Submitting…" : "Submit answers"}
            </button>
          ) : null}
        </div>
      )}
    </section>
  );
}
