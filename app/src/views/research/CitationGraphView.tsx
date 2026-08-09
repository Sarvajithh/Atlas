import { useEffect, useMemo, useState } from "react";

import { graphCitationGraph } from "@/ipc/graph";
import type { CitationGraphEdge, Workspace } from "@/ipc/types";

/**
 * Citation Graph view (V1.0 Part 2, § objective "citation graph /
 * cross-document linking"). Every edge shown is a real Concept Graph edge
 * whose endpoints are, between them, sourced from more than one document
 * (`graph.citationGraph`) -- there is no separate/mock relationship data
 * here, and a within-one-document relation is intentionally excluded
 * (that's what makes this a *citation* graph rather than just the full
 * Concept Graph, which is `graph.get`/`ConceptGraphView`).
 *
 * Previously a grouped `<ul>` list, deliberately deferred ("a visual
 * graph layout is a reasonable follow-up"). That follow-up is this
 * change: a real SVG node-link diagram over the same real edges,
 * following the same circular-layout approach already shipped for
 * `ConceptGraphView` (deterministic, legible at the tens-of-nodes scale
 * this realistically reaches -- not a force-directed layout library).
 * The per-edge document provenance (source_document_ids) that the old
 * list surfaced is preserved in the node detail panel, not dropped.
 */

interface GraphNode {
  id: number;
  label: string;
  x: number;
  y: number;
}

function layoutCircular(nodes: { id: number; label: string }[], width: number, height: number): GraphNode[] {
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(width, height) / 2 - 60;
  return nodes.map((node, i) => {
    const angle = (2 * Math.PI * i) / Math.max(nodes.length, 1) - Math.PI / 2;
    return { ...node, x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  });
}

export function CitationGraphView({ workspaces }: { workspaces: Workspace[] }) {
  const [selectedWorkspaceIds, setSelectedWorkspaceIds] = useState<number[]>(workspaces.map((w) => w.id));
  const [edges, setEdges] = useState<CitationGraphEdge[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);

  useEffect(() => {
    if (selectedWorkspaceIds.length === 0) {
      setEdges([]);
      return;
    }
    let cancelled = false;
    setIsLoading(true);
    setError(null);
    setSelectedNodeId(null);
    graphCitationGraph(selectedWorkspaceIds)
      .then((result) => {
        if (!cancelled) setEdges(result);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedWorkspaceIds]);

  function toggle(id: number) {
    setSelectedWorkspaceIds((current) =>
      current.includes(id) ? current.filter((existing) => existing !== id) : [...current, id],
    );
  }

  const width = 640;
  const height = 480;

  // Real nodes derived from the real edges -- no separate node fetch and
  // no invented labels: every node here is one endpoint of a genuine
  // citation edge, deduplicated by concept id.
  const nodesById = useMemo(() => {
    const map = new Map<number, { id: number; label: string }>();
    for (const entry of edges) {
      if (!map.has(entry.edge.from_node_id)) {
        map.set(entry.edge.from_node_id, { id: entry.edge.from_node_id, label: entry.from_label });
      }
      if (!map.has(entry.edge.to_node_id)) {
        map.set(entry.edge.to_node_id, { id: entry.edge.to_node_id, label: entry.to_label });
      }
    }
    return map;
  }, [edges]);

  const laidOut = useMemo(
    () => layoutCircular(Array.from(nodesById.values()), width, height),
    [nodesById],
  );
  const laidOutById = useMemo(() => new Map(laidOut.map((n) => [n.id, n])), [laidOut]);
  const selectedNode = selectedNodeId !== null ? laidOutById.get(selectedNodeId) ?? null : null;
  const selectedNodeEdges = useMemo(
    () =>
      selectedNodeId === null
        ? []
        : edges.filter((e) => e.edge.from_node_id === selectedNodeId || e.edge.to_node_id === selectedNodeId),
    [edges, selectedNodeId],
  );

  return (
    <div className="flex flex-col gap-3">
      <fieldset className="flex flex-wrap gap-2" aria-label="Workspaces to include">
        {workspaces.map((workspace) => (
          <label
            key={workspace.id}
            className="flex cursor-pointer items-center gap-1.5 rounded border px-2 py-1 text-sm has-[:checked]:border-primary has-[:checked]:bg-accent"
          >
            <input
              type="checkbox"
              checked={selectedWorkspaceIds.includes(workspace.id)}
              onChange={() => toggle(workspace.id)}
              className="h-3.5 w-3.5"
            />
            {workspace.display_name}
          </label>
        ))}
      </fieldset>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {isLoading ? (
        <p className="text-sm text-muted-foreground">Loading citation graph...</p>
      ) : selectedWorkspaceIds.length === 0 ? (
        <p className="text-sm text-muted-foreground">Select at least one workspace.</p>
      ) : edges.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No cross-document relationships found yet for the selected workspaces. This grows as more documents are
          indexed and their concepts overlap.
        </p>
      ) : (
        <div className="flex flex-col gap-3 lg:flex-row">
          <svg
            viewBox={`0 0 ${width} ${height}`}
            role="img"
            aria-label="Citation graph diagram"
            className="w-full max-w-2xl rounded-md border bg-background"
          >
            {edges.map((entry) => {
              const from = laidOutById.get(entry.edge.from_node_id);
              const to = laidOutById.get(entry.edge.to_node_id);
              if (!from || !to) return null;
              return (
                <line
                  key={entry.edge.id}
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
                <ul className="mt-3 flex flex-col gap-2">
                  {selectedNodeEdges.map((entry) => {
                    const otherId =
                      entry.edge.from_node_id === selectedNode.id ? entry.edge.to_node_id : entry.edge.from_node_id;
                    const otherLabel =
                      entry.edge.from_node_id === selectedNode.id ? entry.to_label : entry.from_label;
                    const direction = entry.edge.from_node_id === selectedNode.id ? "→" : "←";
                    return (
                      <li key={entry.edge.id} className="text-xs">
                        <span className="text-muted-foreground">
                          {direction} {relationLabel(entry.edge.relation_type)}{" "}
                        </span>
                        <span className="font-medium">{otherLabel}</span>
                        <span className="text-muted-foreground"> (#{otherId})</span>
                        <div className="mt-0.5 text-muted-foreground">
                          Sourced from {entry.source_document_ids.length} document(s) (
                          {entry.source_document_ids.map((id) => `#${id}`).join(", ")})
                        </div>
                      </li>
                    );
                  })}
                  {selectedNodeEdges.length === 0 ? (
                    <li className="text-xs text-muted-foreground">No citation relations recorded for this concept.</li>
                  ) : null}
                </ul>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">Click a node to see its cross-document relations.</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function relationLabel(relationType: string): string {
  switch (relationType) {
    case "PrerequisiteOf":
      return "is a prerequisite of";
    case "PartOf":
      return "is part of";
    case "RelatedTo":
    default:
      return "relates to";
  }
}
