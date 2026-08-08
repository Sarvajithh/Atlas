/**
 * Quiz / Exam Mode (§8.2.5): full-focus mode, Assistant Panel hidden or
 * restricted (§8.3). Previously a bare stub (`<section />`, no logic),
 * which is why it rendered blank despite `assistant.quiz` /
 * `assistant.flashcards` (`app-tauri/src/commands/assistant.rs`,
 * `assistant_quiz` / `assistant_flashcards`, real Reasoning-engine calls
 * through `AppFacade::quiz` / `AppFacade::flashcards`) already existing
 * and already having frontend wrappers in `ipc/assistant.ts` that were
 * simply never called from any component.
 */
import { useState } from "react";

import { assistantFlashcards, assistantQuiz } from "@/ipc/assistant";
import { useAppStore } from "@/state/store";
import type { GeneratedContent } from "@/ipc/types";
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
  const [result, setResult] = useState<GeneratedContent | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleGenerate() {
    if (workspaceId === null || !topic.trim()) return;
    setIsLoading(true);
    setError(null);
    setResult(null);
    try {
      const content =
        mode === "quiz" ? await assistantQuiz(workspaceId, topic) : await assistantFlashcards(workspaceId, topic);
      setResult(content);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
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
      ) : result ? (
        <div className="rounded-md border p-3">
          <pre className="whitespace-pre-wrap text-sm">{result.content}</pre>
          {result.citations.length > 0 ? (
            <p className="mt-2 text-xs text-muted-foreground">{result.citations.length} source citation(s)</p>
          ) : null}
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
