import { useEffect, useState } from "react";

import { graphCitationGraph } from "@/ipc/graph";
import type { CitationGraphEdge, Workspace } from "@/ipc/types";

/**
 * Citation Graph view (§ objective "citation graph / cross-document
 * linking"). Every edge shown is a real Concept Graph edge whose
 * endpoints are, between them, sourced from more than one document
 * (`graph.citationGraph`) -- there is no separate/mock relationship data
 * here, and a within-one-document relation is intentionally excluded
 * (that's what makes this a *citation* graph rather than just the full
 * Concept Graph, which is `graph.get`/`ConceptGraphView`).
 *
 * Rendered as a simple grouped list rather than a node-link diagram --
 * this milestone focuses on the query being real and correct; a visual
 * graph layout is a reasonable follow-up, not a blocker for the data
 * being right.
 */
export function CitationGraphView({ workspaces }: { workspaces: Workspace[] }) {
  const [selectedWorkspaceIds, setSelectedWorkspaceIds] = useState<number[]>(workspaces.map((w) => w.id));
  const [edges, setEdges] = useState<CitationGraphEdge[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (selectedWorkspaceIds.length === 0) {
      setEdges([]);
      return;
    }
    let cancelled = false;
    setIsLoading(true);
    setError(null);
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
        <ul className="flex flex-col gap-2">
          {edges.map((entry) => (
            <li key={entry.edge.id} className="rounded border p-2 text-sm">
              <span className="font-medium">{entry.from_label}</span>{" "}
              <span className="text-muted-foreground">{relationLabel(entry.edge.relation_type)}</span>{" "}
              <span className="font-medium">{entry.to_label}</span>
              <div className="mt-1 text-xs text-muted-foreground">
                Sourced from {entry.source_document_ids.length} documents (
                {entry.source_document_ids.map((id) => `#${id}`).join(", ")})
              </div>
            </li>
          ))}
        </ul>
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
