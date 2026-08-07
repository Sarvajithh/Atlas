import { useEffect, useMemo, useState } from "react";

import { graphGet, graphGetEdges } from "@/ipc/graph";
import type { ConceptEdge, ConceptNode } from "@/ipc/types";
import { useAppStore } from "@/state/store";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

const RELATION_LABEL: Record<ConceptEdge["relation_type"], string> = {
  PrerequisiteOf: "is a prerequisite of",
  RelatedTo: "is related to",
  PartOf: "is part of",
};

/**
 * Concept Graph View (§8.2.3): nodes/edges extracted by the Phase 5
 * extraction pipeline (`atlas-graph::GraphEngine::extract_for_document`),
 * queried read-only via `graph.get`/`graph.getEdges` (§43.1) and filtered
 * by the active workspace. Full force-directed graph rendering is a later
 * UI-implementation milestone; this renders every node with its outgoing
 * relations as a real, data-backed list rather than a placeholder.
 */
export function ConceptGraphView() {
  const activeWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const workspaces = useAppStore((s) => s.workspaces);

  const [nodes, setNodes] = useState<ConceptNode[]>([]);
  const [edges, setEdges] = useState<ConceptEdge[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  async function load(workspaceId: number) {
    setLoading(true);
    setError(null);
    try {
      const [nodeList, edgeList] = await Promise.all([
        graphGet(workspaceId),
        graphGetEdges(workspaceId),
      ]);
      setNodes(nodeList);
      setEdges(edgeList);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (activeWorkspaceId != null) {
      void load(activeWorkspaceId);
    } else {
      setNodes([]);
      setEdges([]);
    }
    setSelectedId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeWorkspaceId]);

  const nodeById = useMemo(() => new Map(nodes.map((n) => [n.id, n])), [nodes]);
  const edgesByFromNode = useMemo(() => {
    const map = new Map<number, ConceptEdge[]>();
    for (const edge of edges) {
      const list = map.get(edge.from_node_id) ?? [];
      list.push(edge);
      map.set(edge.from_node_id, list);
    }
    return map;
  }, [edges]);

  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const selectedNode = selectedId != null ? nodeById.get(selectedId) ?? null : null;

  return (
    <section aria-label="Concept Graph View" className="flex h-full flex-col overflow-auto p-6">
      <div className="mb-4">
        <h1 className="text-xl font-semibold">Concept Graph</h1>
        <p className="text-sm text-muted-foreground">
          {activeWorkspace ? `Concepts extracted from ${activeWorkspace.display_name}` : "Select a workspace to view its concept graph."}
        </p>
      </div>

      {activeWorkspaceId == null ? (
        <EmptyState
          title="No workspace selected"
          description="Open a workspace to see the concepts Atlas has extracted from its documents."
        />
      ) : loading ? (
        <LoadingState label="Loading concept graph…" />
      ) : error ? (
        <ErrorState message={error} onRetry={() => load(activeWorkspaceId)} />
      ) : nodes.length === 0 ? (
        <EmptyState
          title="No concepts yet"
          description="Concepts are extracted automatically in the background after documents finish indexing. Check back once indexing completes."
        />
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <ul className="lg:col-span-2 flex flex-col gap-2">
            {nodes.map((node) => {
              const outgoing = edgesByFromNode.get(node.id) ?? [];
              return (
                <li key={node.id}>
                  <button
                    type="button"
                    onClick={() => setSelectedId(node.id)}
                    className={`w-full rounded-lg border p-3 text-left hover:border-primary hover:bg-accent/40 ${
                      selectedId === node.id ? "border-primary bg-accent/40" : ""
                    }`}
                  >
                    <div className="font-medium">{node.label}</div>
                    {node.description ? (
                      <p className="mt-1 text-xs text-muted-foreground">{node.description}</p>
                    ) : null}
                    {outgoing.length > 0 ? (
                      <ul className="mt-2 flex flex-col gap-0.5 text-xs text-muted-foreground">
                        {outgoing.map((edge) => (
                          <li key={edge.id}>
                            {RELATION_LABEL[edge.relation_type]} {nodeById.get(edge.to_node_id)?.label ?? "…"}
                          </li>
                        ))}
                      </ul>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>

          <aside className="rounded-lg border p-4">
            {selectedNode ? (
              <div>
                <h2 className="font-medium">{selectedNode.label}</h2>
                {selectedNode.description ? (
                  <p className="mt-1 text-sm text-muted-foreground">{selectedNode.description}</p>
                ) : (
                  <p className="mt-1 text-sm text-muted-foreground">No description extracted.</p>
                )}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">Select a concept to see its details.</p>
            )}
          </aside>
        </div>
      )}
    </section>
  );
}
