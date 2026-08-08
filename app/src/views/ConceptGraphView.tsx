/**
 * Concept Graph View (§8.2.3): visual graph of extracted concepts and
 * their relations, filterable by workspace.
 *
 * Previously this rendered nodes as a flat `<ul>` list -- not because the
 * data wasn't there, but because `graph_get`/`graph.get` never returned
 * anything but nodes; there was no IPC call that fetched a whole
 * workspace's edges at once (`GraphRepository::list_edges_for_node` only
 * ever looked up one node's edges). That's fixed (`graph.getFull`,
 * `AppFacade::graph_full`), so this now lays nodes out on a circle and
 * draws each real edge between them as an SVG line -- a real, if simple,
 * node-link diagram, not a force-directed layout library. There's also no
 * way for a user to ask for extraction to run again (e.g. after a first
 * pass under-extracted, or after editing indexed documents); `graph.reextract`
 * (`AppFacade::reextract_workspace_concepts`) closes that gap too.
 */
import { useCallback, useEffect, useMemo, useState } from "react";

import { graphGetFull, graphReextract } from "@/ipc/graph";
import { useAppStore } from "@/state/store";
import type { ConceptEdge, ConceptNode, ExtractionOutcome } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

const RELATION_LABEL: Record<ConceptEdge["relation_type"], string> = {
  PrerequisiteOf: "prerequisite of",
  RelatedTo: "related to",
  PartOf: "part of",
};

interface LaidOutNode extends ConceptNode {
  x: number;
  y: number;
}

/** Places nodes evenly around a circle -- simple, deterministic, and
 * legible for the tens-of-nodes scale a single workspace's Concept Graph
 * realistically reaches; a real force-directed layout is more than this
 * fix's scope needs. */
function layoutCircular(nodes: ConceptNode[], width: number, height: number): LaidOutNode[] {
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(width, height) / 2 - 60;
  return nodes.map((node, i) => {
    const angle = (2 * Math.PI * i) / Math.max(nodes.length, 1) - Math.PI / 2;
    return { ...node, x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  });
}

export function ConceptGraphView() {
  const workspaces = useAppStore((s) => s.workspaces);
  const storeActiveWorkspaceId = useAppStore((s) => s.activeWorkspaceId);
  const [workspaceId, setWorkspaceId] = useState<number | null>(
    storeActiveWorkspaceId ?? workspaces[0]?.id ?? null,
  );
  const [nodes, setNodes] = useState<ConceptNode[] | null>(null);
  const [edges, setEdges] = useState<ConceptEdge[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);

  const [isReextracting, setIsReextracting] = useState(false);
  const [reextractResult, setReextractResult] = useState<ExtractionOutcome | null>(null);
  const [reextractError, setReextractError] = useState<string | null>(null);

  const load = useCallback(async (id: number) => {
    setError(null);
    setNodes(null);
    setEdges([]);
    setSelectedNodeId(null);
    try {
      const result = await graphGetFull(id);
      setNodes(result.nodes);
      setEdges(result.edges);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    if (workspaceId !== null) void load(workspaceId);
  }, [workspaceId, load]);

  async function handleReextract() {
    if (workspaceId === null) return;
    setIsReextracting(true);
    setReextractError(null);
    setReextractResult(null);
    try {
      const outcome = await graphReextract(workspaceId);
      setReextractResult(outcome);
      await load(workspaceId);
    } catch (err) {
      setReextractError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsReextracting(false);
    }
  }

  const width = 640;
  const height = 480;
  const laidOut = useMemo(() => layoutCircular(nodes ?? [], width, height), [nodes]);
  const nodeById = useMemo(() => new Map(laidOut.map((n) => [n.id, n])), [laidOut]);
  const selectedNode = selectedNodeId !== null ? nodeById.get(selectedNodeId) ?? null : null;

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
      <div className="flex flex-wrap items-center gap-2">
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

        <button
          type="button"
          onClick={handleReextract}
          disabled={workspaceId === null || isReextracting}
          className="ml-auto rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-50"
        >
          {isReextracting ? "Re-extracting…" : "Re-extract concepts"}
        </button>
      </div>

      {reextractError ? <ErrorState message={reextractError} onRetry={handleReextract} /> : null}
      {reextractResult ? (
        <p className="text-xs text-muted-foreground">
          Re-extraction complete: {reextractResult.nodes_created} concept(s) created,{" "}
          {reextractResult.nodes_reused} reused, {reextractResult.edges_created} relation(s) created,{" "}
          {reextractResult.edges_skipped_existing} already present.
        </p>
      ) : null}

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
        <div className="flex flex-col gap-3 lg:flex-row">
          <svg
            viewBox={`0 0 ${width} ${height}`}
            role="img"
            aria-label="Concept graph diagram"
            className="w-full max-w-2xl rounded-md border bg-background"
          >
            {edges.map((edge) => {
              const from = nodeById.get(edge.from_node_id);
              const to = nodeById.get(edge.to_node_id);
              if (!from || !to) return null;
              return (
                <line
                  key={edge.id}
                  x1={from.x}
                  y1={from.y}
                  x2={to.x}
                  y2={to.y}
                  stroke="currentColor"
                  strokeOpacity={0.35}
                  strokeWidth={1.5}
                  className="text-muted-foreground"
                />
              );
            })}
            {laidOut.map((node) => (
              <g
                key={node.id}
                transform={`translate(${node.x}, ${node.y})`}
                onClick={() => setSelectedNodeId(node.id)}
                className="cursor-pointer"
              >
                <circle
                  r={selectedNodeId === node.id ? 10 : 7}
                  className={selectedNodeId === node.id ? "fill-primary" : "fill-accent stroke-border"}
                  strokeWidth={1}
                />
                <text x={12} y={4} fontSize={11} className="fill-foreground">
                  {node.label}
                </text>
              </g>
            ))}
          </svg>

          <div className="flex-1 rounded-md border p-3">
            {selectedNode ? (
              <>
                <p className="text-sm font-medium">{selectedNode.label}</p>
                {selectedNode.description ? (
                  <p className="mt-1 text-xs text-muted-foreground">{selectedNode.description}</p>
                ) : null}
                <ul className="mt-3 flex flex-col gap-1">
                  {edges
                    .filter((e) => e.from_node_id === selectedNode.id || e.to_node_id === selectedNode.id)
                    .map((e) => {
                      const otherId = e.from_node_id === selectedNode.id ? e.to_node_id : e.from_node_id;
                      const other = nodeById.get(otherId);
                      const direction = e.from_node_id === selectedNode.id ? "→" : "←";
                      return (
                        <li key={e.id} className="text-xs text-muted-foreground">
                          {direction} {RELATION_LABEL[e.relation_type]} {other?.label ?? `#${otherId}`}
                        </li>
                      );
                    })}
                  {edges.filter((e) => e.from_node_id === selectedNode.id || e.to_node_id === selectedNode.id)
                    .length === 0 ? (
                    <li className="text-xs text-muted-foreground">No relations recorded for this concept.</li>
                  ) : null}
                </ul>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">Click a node to see its relations.</p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
