/**
 * Research Mode (§8.2.6): multi-document, cross-workspace assistant
 * context for literature-review-style work (§9: explicit, opt-in state).
 *
 * Literature Review, Paper Comparison, and Citation Graph are real here --
 * every panel below is backed by a genuine IPC round-trip
 * (`rag.researchQuery`, `graph.citationGraph`) over real retrieved/
 * extracted data, no mock content.
 *
 * Timeline is intentionally NOT implemented in this phase and is flagged
 * here rather than silently missing: `DocumentRecord` (§33.2) carries only
 * a filesystem `mtime`, not a publication/authored date, so there is no
 * real chronological metadata to surface yet. Fabricating one from file
 * modification time would be actively misleading (a re-saved or
 * re-indexed older paper would sort as "recent"). Building a real one
 * needs parser-level date extraction (e.g. a PDF's metadata date, or a
 * document's own "Published on ..." text) -- deeper parser work than this
 * phase's scope, called out here as the explicit follow-up rather than
 * skipped without comment.
 */
import { useState } from "react";
import type { ReactNode } from "react";

import { useAppStore } from "@/state/store";

import { CitationGraphView } from "@/views/research/CitationGraphView";
import { ResearchQueryPanel } from "@/views/research/ResearchQueryPanel";

type ResearchTab = "literature" | "citations" | "timeline";

export function ResearchMode() {
  const workspaces = useAppStore((s) => s.workspaces);
  const [tab, setTab] = useState<ResearchTab>("literature");

  return (
    <section aria-label="Research Mode" className="flex h-full flex-col gap-4 overflow-auto p-4">
      <nav className="flex items-center gap-1 border-b text-sm" aria-label="Research Mode sections">
        <TabButton active={tab === "literature"} onClick={() => setTab("literature")}>
          Literature review &amp; comparison
        </TabButton>
        <TabButton active={tab === "citations"} onClick={() => setTab("citations")}>
          Citation graph
        </TabButton>
        <TabButton active={tab === "timeline"} onClick={() => setTab("timeline")}>
          Timeline
        </TabButton>
      </nav>

      {tab === "literature" && <ResearchQueryPanel workspaces={workspaces} />}
      {tab === "citations" && <CitationGraphView workspaces={workspaces} />}
      {tab === "timeline" && (
        <p className="text-sm text-muted-foreground">
          Timeline is deferred: source documents don't yet carry a real publication/authored date (only filesystem
          modification time), so there's no genuine chronological data to show. See the note in this file's doc
          comment for what a real implementation needs.
        </p>
      )}
    </section>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className="border-b-2 border-transparent px-3 py-2 hover:bg-accent aria-[pressed=true]:border-primary aria-[pressed=true]:font-medium"
    >
      {children}
    </button>
  );
}
