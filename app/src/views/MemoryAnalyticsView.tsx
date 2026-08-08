/**
 * Memory & Analytics View (§8.2.4): weakness tracking, revision planner,
 * mastery over time. Previously a bare stub (`<section />`, no logic),
 * which is why it rendered blank despite `atlas-memory`'s
 * progress/analytics repositories being real and backed by SQLite (see
 * README "Student Memory").
 *
 * Only per-concept progress IPC exists today (`memory.getWeaknesses`,
 * one `LearningProgress | null` per `conceptNodeId` -- there is no
 * aggregate "list all progress rows for a workspace" command). This view
 * is honest about that constraint: it lists the workspace's real concept
 * nodes (`graph.get`, the same source ConceptGraphView uses) and fetches
 * real mastery/weakness data per concept, rather than fabricating an
 * aggregate dashboard the backend can't actually answer yet. A concept
 * with no recorded progress shows "not yet reviewed", not a fake score.
 */
import { useEffect, useState } from "react";

import { graphGet } from "@/ipc/graph";
import { memoryGetWeaknesses } from "@/ipc/memory";
import { useAppStore } from "@/state/store";
import type { ConceptNode, LearningProgress } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

export function MemoryAnalyticsView() {
  const workspaces = useAppStore((s) => s.workspaces);
  const storeActiveWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const [workspaceId, setWorkspaceId] = useState<number | null>(
    storeActiveWorkspaceId ?? workspaces[0]?.id ?? null,
  );
  const [nodes, setNodes] = useState<ConceptNode[] | null>(null);
  const [progress, setProgress] = useState<Record<number, LearningProgress | null>>({});
  const [error, setError] = useState<string | null>(null);

  async function load(id: number) {
    setError(null);
    setNodes(null);
    setProgress({});
    try {
      const conceptNodes = await graphGet(id);
      setNodes(conceptNodes);
      const entries = await Promise.all(
        conceptNodes.map(async (n) => [n.id, await memoryGetWeaknesses(n.id)] as const),
      );
      setProgress(Object.fromEntries(entries));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    if (workspaceId !== null) void load(workspaceId);
  }, [workspaceId]);

  if (workspaces.length === 0) {
    return (
      <section aria-label="Memory and Analytics View" className="flex h-full flex-col p-4">
        <EmptyState title="No workspaces yet" description="Link a workspace to start tracking progress." />
      </section>
    );
  }

  return (
    <section aria-label="Memory and Analytics View" className="flex h-full flex-col gap-3 overflow-auto p-4">
      <div className="flex items-center gap-2">
        <label htmlFor="memory-analytics-workspace" className="text-sm text-muted-foreground">
          Workspace
        </label>
        <select
          id="memory-analytics-workspace"
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

      {error ? (
        <ErrorState message={error} onRetry={() => workspaceId !== null && load(workspaceId)} />
      ) : nodes === null ? (
        <LoadingState label="Loading progress data…" />
      ) : nodes.length === 0 ? (
        <EmptyState
          title="No concepts to track yet"
          description="Progress and mastery tracking is per-concept. Once documents in this workspace have indexed and concepts have been extracted, they'll show up here."
        />
      ) : (
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b text-xs text-muted-foreground">
              <th className="py-1 pr-2 font-medium">Concept</th>
              <th className="py-1 pr-2 font-medium">Mastery</th>
              <th className="py-1 pr-2 font-medium">Weakness</th>
              <th className="py-1 pr-2 font-medium">Attempts</th>
              <th className="py-1 pr-2 font-medium">Last reviewed</th>
            </tr>
          </thead>
          <tbody>
            {nodes.map((node) => {
              const p = progress[node.id];
              return (
                <tr key={node.id} className="border-b last:border-0">
                  <td className="py-1.5 pr-2">{node.label}</td>
                  <td className="py-1.5 pr-2">{p ? `${Math.round(p.mastery_score * 100)}%` : "—"}</td>
                  <td className="py-1.5 pr-2">{p ? `${Math.round(p.weakness_score * 100)}%` : "—"}</td>
                  <td className="py-1.5 pr-2">{p ? p.attempt_count : 0}</td>
                  <td className="py-1.5 pr-2 text-muted-foreground">
                    {p?.last_reviewed_at ? new Date(p.last_reviewed_at).toLocaleDateString() : "Not yet reviewed"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
  );
}
