/**
 * Quiz / Exam Mode (§8.2.5): full-focus mode, Assistant Panel hidden or
 * restricted (§8.3).
 *
 * Previously rendered `assistant_quiz`/`assistant_flashcards` output as a
 * single `<pre>`-dumped prose blob with no structure -- no individual
 * questions, no answer capture, no grading, and no link into Student
 * Memory. The backend now returns real structured `GeneratedQuiz`/
 * `GeneratedFlashcards` data (§43.2, `atlas_types::quiz`) and a real
 * `assistant_quiz_submit` grading command, so this view now renders an
 * actual multiple-choice quiz flow (select an answer per question, submit,
 * see per-question correctness + score) and a front/back flashcard review
 * flow, instead of a passive text dump.
 */
import { useState } from "react";

import { assistantFlashcards, assistantQuiz, assistantQuizSubmit } from "@/ipc/assistant";
import { useAppStore } from "@/state/store";
import type { GeneratedFlashcards, GeneratedQuiz, QuizGradeResult } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

type Mode = "quiz" | "flashcards";

export function QuizExamMode() {
  const workspaces = useAppStore((s) => s.workspaces);
  const storeActiveWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const [workspaceId, setWorkspaceId] = useState<number | null>(
    storeActiveWorkspaceId ?? workspaces[0]?.id ?? null,
  );
  const [mode, setMode] = useState<Mode>("quiz");
  const [topic, setTopic] = useState("");

  const [quiz, setQuiz] = useState<GeneratedQuiz | null>(null);
  const [flashcards, setFlashcards] = useState<GeneratedFlashcards | null>(null);
  const [answers, setAnswers] = useState<(number | null)[]>([]);
  const [grade, setGrade] = useState<QuizGradeResult | null>(null);
  const [flippedCard, setFlippedCard] = useState<number | null>(null);

  const [isLoading, setIsLoading] = useState(false);
  const [isGrading, setIsGrading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleGenerate() {
    if (workspaceId === null || !topic.trim()) return;
    setIsLoading(true);
    setError(null);
    setQuiz(null);
    setFlashcards(null);
    setGrade(null);
    setFlippedCard(null);
    try {
      if (mode === "quiz") {
        const generated = await assistantQuiz(workspaceId, topic);
        setQuiz(generated);
        setAnswers(new Array(generated.questions.length).fill(null));
      } else {
        setFlashcards(await assistantFlashcards(workspaceId, topic));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }

  function selectAnswer(questionIndex: number, optionIndex: number) {
    if (grade) return; // locked once graded
    setAnswers((prev) => {
      const next = [...prev];
      next[questionIndex] = optionIndex;
      return next;
    });
  }

  async function handleSubmitQuiz() {
    if (workspaceId === null || !quiz) return;
    setIsGrading(true);
    setError(null);
    try {
      const result = await assistantQuizSubmit(workspaceId, quiz.topic, quiz.questions, answers);
      setGrade(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsGrading(false);
    }
  }

  if (workspaces.length === 0) {
    return (
      <section aria-label="Quiz and Exam Mode" className="flex h-full flex-col p-4">
        <EmptyState title="No workspaces yet" description="Link a workspace with indexed documents to generate quizzes." />
      </section>
    );
  }

  return (
    <section aria-label="Quiz and Exam Mode" className="flex h-full flex-col gap-4 overflow-auto p-4">
      <div className="flex flex-wrap items-end gap-2">
        <div className="flex flex-col gap-1">
          <label htmlFor="quiz-workspace" className="text-xs text-muted-foreground">
            Workspace
          </label>
          <select
            id="quiz-workspace"
            value={workspaceId ?? ""}
            onChange={(e) => setWorkspaceId(Number(e.target.value))}
            className="rounded-md border bg-background px-2 py-1 text-sm"
          >
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.display_name}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">Mode</span>
          <div className="flex gap-1">
            <button
              type="button"
              onClick={() => setMode("quiz")}
              className={`rounded-md border px-2 py-1 text-sm ${mode === "quiz" ? "bg-accent" : ""}`}
            >
              Quiz
            </button>
            <button
              type="button"
              onClick={() => setMode("flashcards")}
              className={`rounded-md border px-2 py-1 text-sm ${mode === "flashcards" ? "bg-accent" : ""}`}
            >
              Flashcards
            </button>
          </div>
        </div>

        <div className="flex flex-1 flex-col gap-1">
          <label htmlFor="quiz-topic" className="text-xs text-muted-foreground">
            Topic
          </label>
          <input
            id="quiz-topic"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder="e.g. photosynthesis"
            className="rounded-md border bg-background px-2 py-1 text-sm"
          />
        </div>

        <button
          type="button"
          onClick={handleGenerate}
          disabled={!topic.trim() || workspaceId === null || isLoading}
          className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-50"
        >
          Generate
        </button>
      </div>

      {isLoading ? (
        <LoadingState label={mode === "quiz" ? "Generating quiz…" : "Generating flashcards…"} />
      ) : error ? (
        <ErrorState message={error} onRetry={handleGenerate} />
      ) : mode === "quiz" && quiz ? (
        <div className="flex flex-col gap-4">
          {grade ? (
            <div className="rounded-md border p-3 text-sm font-medium">
              Score: {grade.correct_count} / {grade.total_count} ({Math.round(grade.score * 100)}%)
              {grade.matched_concept_node_id !== null ? (
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                  Saved to this concept's progress.
                </span>
              ) : (
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                  No matching concept found for "{quiz.topic}" — score not saved to progress.
                </span>
              )}
            </div>
          ) : null}

          {quiz.questions.map((q, qi) => {
            const result = grade?.results.find((r) => r.question_index === qi);
            return (
              <div key={qi} className="rounded-md border p-3">
                <p className="mb-2 text-sm font-medium">
                  {qi + 1}. {q.question}
                </p>
                <div className="flex flex-col gap-1">
                  {q.options.map((option, oi) => {
                    const isSelected = answers[qi] === oi;
                    const isCorrectOption = grade ? oi === result?.correct_index : false;
                    let optionClass = "border-border";
                    if (grade) {
                      if (isCorrectOption) optionClass = "border-green-600 bg-green-600/10";
                      else if (isSelected && !isCorrectOption) optionClass = "border-red-600 bg-red-600/10";
                    } else if (isSelected) {
                      optionClass = "border-primary bg-accent";
                    }
                    return (
                      <button
                        key={oi}
                        type="button"
                        onClick={() => selectAnswer(qi, oi)}
                        disabled={!!grade}
                        className={`rounded-md border px-2 py-1 text-left text-sm disabled:opacity-90 ${optionClass}`}
                      >
                        {option}
                      </button>
                    );
                  })}
                </div>
                {grade && result ? (
                  <p className="mt-2 text-xs text-muted-foreground">{q.explanation}</p>
                ) : null}
              </div>
            );
          })}

          {!grade ? (
            <button
              type="button"
              onClick={handleSubmitQuiz}
              disabled={isGrading}
              className="self-start rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-50"
            >
              {isGrading ? "Grading…" : "Submit answers"}
            </button>
          ) : null}
        </div>
      ) : mode === "flashcards" && flashcards ? (
        <div className="grid gap-3 sm:grid-cols-2">
          {flashcards.cards.map((card, i) => (
            <button
              key={i}
              type="button"
              onClick={() => setFlippedCard(flippedCard === i ? null : i)}
              className="flex min-h-[100px] flex-col justify-center rounded-md border p-3 text-left text-sm hover:bg-accent"
            >
              <span className="mb-1 text-xs text-muted-foreground">
                {flippedCard === i ? "Answer (click to flip back)" : "Question (click to flip)"}
              </span>
              {flippedCard === i ? card.back : card.front}
            </button>
          ))}
        </div>
      ) : (
        <EmptyState
          title="No quiz generated yet"
          description="Enter a topic and generate to produce a real quiz or flashcard set from this workspace's indexed content."
        />
      )}
    </section>
  );
}
