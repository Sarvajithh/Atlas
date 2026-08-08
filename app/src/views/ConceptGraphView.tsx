/**
 * Concept Graph View (§8.2.3): visual graph of extracted concepts and
 * their relations, filterable by workspace. Backed by real `graph.get`
 * (§43.1) data -- previously this component was a bare stub (`<section />`
 * with no logic at all), which is why it rendered as a silent blank pane
 * despite `atlas-graph::extraction` genuinely populating nodes for every
 * indexed workspace (see README "Concept Graph"). Node-link *rendering* is
 * still out of scope (matches Research Mode's Citation Graph, which is
 * also a grouped list, not a canvas) -- this phase's fix is making the
 * real data actually reach the screen, honestly, not adding graph layout.
 */
import { useEffect, useState } from "react";

import { graphGet } from "@/ipc/graph";
import { useAppStore } from "@/state/store";
import type { ConceptNode } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

export function ConceptGraphView() {
  const workspaces = useAppStore((s) => s.workspaces);
  const storeActiveWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const [workspaceId, setWorkspaceId] = useState<number | null>(
    storeActiveWorkspaceId ?? workspaces[0]?.id ?? null,
  );
  const [nodes, setNodes] = useState<ConceptNode[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load(id: number) {
    setError(null);
    setNodes(null);
    try {
      const result = await graphGet(id);
      setNodes(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    if (workspaceId !== null) void load(workspaceId);
  }, [workspaceId]);

  if (workspaces.length === 0) {
    return (
      <section aria-label="Concept Graph View" className="flex h-full flex-col p-4">
        <EmptyState
          title="No workspaces yet"
          description="Link a workspace and index some documents to start extracting concepts."
        />
      </section>
    );
  }

  return (
    <section aria-label="Concept Graph View" className="flex h-full flex-col gap-3 overflow-auto p-4">
      <div className="flex items-center gap-2">
        <label htmlFor="concept-graph-workspace" className="text-sm text-muted-foreground">
          Workspace
        </label>
        <select
          id="concept-graph-workspace"
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
        <LoadingState label="Loading concept graph…" />
      ) : nodes.length === 0 ? (
        <EmptyState
          title="No concepts extracted yet"
          description="Concepts are extracted automatically as documents finish indexing. Once this workspace has indexed documents, extracted concepts and their relations will appear here."
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {nodes.map((node) => (
            <li key={node.id} className="rounded-md border p-3">
              <p className="text-sm font-medium">{node.label}</p>
              {node.description ? (
                <p className="mt-1 text-xs text-muted-foreground">{node.description}</p>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
