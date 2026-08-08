/**
 * Research Mode (§8.2.6): multi-document, cross-workspace assistant
 * context for literature-review-style work (§9: explicit, opt-in state).
 *
 * Literature Review, Paper Comparison, Citation Graph, and Timeline are
 * all real here -- every panel below is backed by a genuine IPC
 * round-trip (`rag.researchQuery`, `graph.citationGraph`, `document.list`)
 * over real retrieved/extracted/parsed data, no mock content.
 *
 * Timeline was previously deferred rather than fabricated:
 * `DocumentRecord` only carried a filesystem `mtime`, not a publication
 * date, and sorting by `mtime` would be actively misleading (a re-saved
 * older paper would sort as "recent"). `DocumentRecord.authored_at` is
 * now a real, best-effort parser-level authored date
 * (`atlas_indexer::dates`); see `ResearchTimelineView`'s doc comment for
 * how it's rendered, including the documents it honestly can't date.
 */
import { useState } from "react";
import type { ReactNode } from "react";

import { useAppStore } from "@/state/store";

import { CitationGraphView } from "@/views/research/CitationGraphView";
import { ResearchQueryPanel } from "@/views/research/ResearchQueryPanel";
import { ResearchTimelineView } from "@/views/research/ResearchTimelineView";

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
      {tab === "timeline" && <ResearchTimelineView workspaces={workspaces} />}
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
