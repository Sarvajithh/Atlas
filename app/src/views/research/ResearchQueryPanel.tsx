import { useState } from "react";

import { ragResearchQuery, type ResearchQueryMode } from "@/ipc/rag";
import type { Citation, SearchResult, Workspace } from "@/ipc/types";

import { WorkspaceMultiSelect } from "@/views/research/WorkspaceMultiSelect";

/**
 * Literature Review / Paper Comparison panel (§ objective "literature
 * review support, paper comparison"). Both tasks share this one panel --
 * they're the same underlying cross-workspace synthesis query
 * (`rag.researchQuery`), differing only in which system prompt framing
 * the backend uses (`mode`), not in retrieval/context assembly. Every
 * answer and every citation shown here comes from a real backend
 * round-trip; nothing is mocked.
 */
export function ResearchQueryPanel({ workspaces }: { workspaces: Workspace[] }) {
  const [selectedWorkspaceIds, setSelectedWorkspaceIds] = useState<number[]>(
    workspaces[0] ? [workspaces[0].id] : [],
  );
  const [mode, setMode] = useState<ResearchQueryMode>("literatureReview");
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<SearchResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canSubmit = query.trim().length > 0 && selectedWorkspaceIds.length > 0 && !isLoading;

  function submit() {
    if (!canSubmit) return;
    setIsLoading(true);
    setError(null);
    ragResearchQuery(selectedWorkspaceIds, query.trim(), mode)
      .then((response) => setResult(response))
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setIsLoading(false));
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <span className="text-xs font-medium text-muted-foreground">Workspaces</span>
        <WorkspaceMultiSelect
          workspaces={workspaces}
          selectedIds={selectedWorkspaceIds}
          onChange={setSelectedWorkspaceIds}
        />
      </div>

      <div className="flex items-center gap-1 text-xs">
        <button
          type="button"
          aria-pressed={mode === "literatureReview"}
          onClick={() => setMode("literatureReview")}
          className="rounded px-2 py-1 hover:bg-accent aria-[pressed=true]:bg-accent"
        >
          Literature review
        </button>
        <button
          type="button"
          aria-pressed={mode === "paperComparison"}
          onClick={() => setMode("paperComparison")}
          className="rounded px-2 py-1 hover:bg-accent aria-[pressed=true]:bg-accent"
        >
          Paper comparison
        </button>
      </div>

      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={
            mode === "literatureReview"
              ? "What does the literature say about..."
              : "Compare how these sources treat..."
          }
          aria-label="Research question"
          className="flex-1 rounded border bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground"
        />
        <button
          type="submit"
          disabled={!canSubmit}
          className="rounded border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-40"
        >
          {isLoading ? "Searching..." : "Ask"}
        </button>
      </form>

      {selectedWorkspaceIds.length === 0 && (
        <p className="text-xs text-muted-foreground">Select at least one workspace to search across.</p>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}

      {result && <ResearchAnswer result={result} />}
    </div>
  );
}

function ResearchAnswer({ result }: { result: SearchResult }) {
  return (
    <div className="rounded border p-3">
      <p className="whitespace-pre-wrap text-sm">{result.content}</p>
      {result.citations.length > 0 && <CitationList citations={result.citations} />}
    </div>
  );
}

function CitationList({ citations }: { citations: Citation[] }) {
  return (
    <ol className="mt-3 flex flex-col gap-1 border-t pt-2 text-xs text-muted-foreground">
      {citations.map((citation, idx) => (
        <li key={`${citation.document_id}-${citation.chunk_id}`}>
          [{idx + 1}] doc #{citation.document_id} ({citation.location_ref}) -- {citation.snippet}
        </li>
      ))}
    </ol>
  );
}
