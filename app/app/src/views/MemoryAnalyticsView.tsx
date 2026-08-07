import { useEffect, useState } from "react";

import { assistantListRevisionPlans, assistantRevisionPlan } from "@/ipc/assistant";
import { memoryListWeakTopics } from "@/ipc/memory";
import type { RevisionPlan, WeakTopic } from "@/ipc/types";
import { useAppStore } from "@/state/store";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

/**
 * Memory & Analytics View (§8.2.4): weakness tracking, revision planner.
 * Wired to the real `memory.list_weak_topics`/`assistant.revision_plan`/
 * `assistant.list_revision_plans` IPC surface (§ Learning subsystem weak-
 * topic detection) -- the weak-topic list below is a real, computed
 * correctness aggregate from recorded quiz attempts (`WeakTopic`), not
 * something re-derived by the model on every view; the revision plan is
 * generated *from* that aggregate.
 */
export function MemoryAnalyticsView() {
  const workspaceId = useAppStore((s) => s.activeWorkspaceId);
  const pushToast = useAppStore((s) => s.pushToast);

  const [weakTopics, setWeakTopics] = useState<WeakTopic[]>([]);
  const [weakTopicsLoading, setWeakTopicsLoading] = useState(false);
  const [weakTopicsError, setWeakTopicsError] = useState<string | null>(null);

  const [plans, setPlans] = useState<RevisionPlan[]>([]);
  const [plansLoading, setPlansLoading] = useState(false);
  const [plansError, setPlansError] = useState<string | null>(null);

  const [generating, setGenerating] = useState(false);

  async function loadWeakTopics() {
    if (workspaceId === null) return;
    setWeakTopicsLoading(true);
    setWeakTopicsError(null);
    try {
      setWeakTopics(await memoryListWeakTopics(workspaceId));
    } catch (err) {
      setWeakTopicsError(err instanceof Error ? err.message : String(err));
    } finally {
      setWeakTopicsLoading(false);
    }
  }

  async function loadPlans() {
    if (workspaceId === null) return;
    setPlansLoading(true);
    setPlansError(null);
    try {
      setPlans(await assistantListRevisionPlans(workspaceId));
    } catch (err) {
      setPlansError(err instanceof Error ? err.message : String(err));
    } finally {
      setPlansLoading(false);
    }
  }

  useEffect(() => {
    void loadWeakTopics();
    void loadPlans();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  async function handleGeneratePlan() {
    if (workspaceId === null) return;
    setGenerating(true);
    try {
      await assistantRevisionPlan(workspaceId);
      void loadPlans();
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    } finally {
      setGenerating(false);
    }
  }

  if (workspaceId === null) {
    return (
      <section aria-label="Memory and Analytics View" className="flex h-full flex-col overflow-auto p-6">
        <EmptyState
          title="No workspace selected"
          description="Open a workspace to see weak-topic tracking and revision plans."
        />
      </section>
    );
  }

  const latestPlan = plans[0] ?? null;

  return (
    <section aria-label="Memory and Analytics View" className="flex h-full flex-col overflow-auto p-6">
      <div className="mb-6">
        <h1 className="text-xl font-semibold">Memory & Analytics</h1>
        <p className="text-sm text-muted-foreground">
          Weak topics are computed from your recorded quiz attempts, weakest first.
        </p>
      </div>

      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Weak topics</h2>
      {weakTopicsLoading ? (
        <LoadingState label="Loading weak topics…" />
      ) : weakTopicsError ? (
        <ErrorState message={weakTopicsError} onRetry={loadWeakTopics} />
      ) : weakTopics.length === 0 ? (
        <EmptyState
          title="No quiz attempts recorded yet"
          description="Take a quiz in Quiz Mode to start building this picture."
        />
      ) : (
        <ul className="mb-8 flex flex-col gap-2">
          {weakTopics.map((topic) => (
            <li key={topic.topic} className="rounded-lg border p-3">
              <div className="mb-1 flex items-center justify-between">
                <span className="text-sm font-medium">{topic.topic}</span>
                <span className="text-xs text-muted-foreground">{Math.round(topic.accuracy * 100)}% correct</span>
              </div>
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full bg-primary"
                  style={{ width: `${Math.round(topic.accuracy * 100)}%` }}
                />
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {topic.correct_count} correct · {topic.incorrect_count} incorrect
              </p>
            </li>
          ))}
        </ul>
      )}

      <div className="mb-2 flex items-center justify-between">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Revision plan</h2>
        <button
          type="button"
          disabled={generating}
          onClick={handleGeneratePlan}
          className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground disabled:opacity-50"
        >
          {generating ? "Generating…" : "Generate new plan"}
        </button>
      </div>
      {plansLoading ? (
        <LoadingState label="Loading revision plans…" />
      ) : plansError ? (
        <ErrorState message={plansError} onRetry={loadPlans} />
      ) : !latestPlan ? (
        <EmptyState
          title="No revision plan yet"
          description="Generate one above once you have some weak-topic data."
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {[...latestPlan.items]
            .sort((a, b) => a.priority - b.priority)
            .map((item, index) => (
              <li key={`${item.topic}-${index}`} className="rounded-lg border p-3">
                <div className="mb-1 flex items-center justify-between">
                  <span className="text-sm font-medium">{item.topic}</span>
                  <span className="text-xs text-muted-foreground">Priority {item.priority}</span>
                </div>
                <p className="text-sm text-muted-foreground">{item.recommendation}</p>
              </li>
            ))}
        </ul>
      )}
    </section>
  );
}
