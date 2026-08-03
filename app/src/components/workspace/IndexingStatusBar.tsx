import { useCallback, useEffect, useRef, useState } from "react";

import { workspaceIndexingStatus, workspaceReindex } from "@/ipc/workspace";
import type { IndexingStatus } from "@/ipc/types";
import { useAppStore } from "@/state/store";

const POLL_INTERVAL_MS = 2000;

/**
 * "Rebuild Workspace Index" button + live status indicator, replacing
 * the fine-tuning idea originally asked for -- the architecture contract
 * explicitly rules out fine-tuning/retraining ("The LLM weights remain
 * unchanged"), so this instead re-runs the existing, already-supported
 * pipeline (Parsing -> OCR -> Chunking -> Embeddings -> Vector DB) for
 * every file in the workspace, and shows the same progress data the
 * Background Indexing Worker already tracks in the `jobs` table
 * (`workspace_indexing_status`, previously wired on the backend but with
 * no frontend consumer).
 */
export function IndexingStatusBar({ workspaceId }: { workspaceId: number }) {
  const pushToast = useAppStore((s) => s.pushToast);
  const [status, setStatus] = useState<IndexingStatus | null>(null);
  const [isRebuilding, setIsRebuilding] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await workspaceIndexingStatus(workspaceId);
      setStatus(next);
      return next;
    } catch {
      // Status polling is supplementary -- a transient failure to fetch
      // it shouldn't surface as a user-facing error.
      return null;
    }
  }, [workspaceId]);

  // Poll continuously while anything is queued/running, and stop once
  // the workspace settles, so this doesn't hit IPC forever for a
  // fully-indexed workspace sitting idle in the background.
  useEffect(() => {
    let cancelled = false;

    async function tick() {
      const next = await refresh();
      if (cancelled) return;
      const active = next !== null && (next.queued > 0 || next.running !== null);
      if (!active && pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    }

    void tick();
    pollRef.current = setInterval(tick, POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      if (pollRef.current) clearInterval(pollRef.current);
      pollRef.current = null;
    };
  }, [refresh]);

  async function handleRebuild() {
    setIsRebuilding(true);
    try {
      const enqueued = await workspaceReindex(workspaceId);
      pushToast({ kind: "success", message: `Rebuilding index: ${enqueued} file(s) queued.` });
      await refresh();
      if (!pollRef.current) {
        pollRef.current = setInterval(async () => {
          const next = await refresh();
          const active = next !== null && (next.queued > 0 || next.running !== null);
          if (!active && pollRef.current) {
            clearInterval(pollRef.current);
            pollRef.current = null;
          }
        }, POLL_INTERVAL_MS);
      }
    } catch (err) {
      pushToast({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    } finally {
      setIsRebuilding(false);
    }
  }

  const isActive = status !== null && (status.queued > 0 || status.running !== null);

  function summary(): string {
    if (!status || status.total === 0) return "Not indexed yet";
    if (isActive) {
      const doneCount = status.succeeded + status.failed;
      const pct = status.progress_percent !== null ? ` (${status.progress_percent.toFixed(0)}%)` : "";
      const current = status.running ? ` — ${status.running.relative_path}` : "";
      return `Indexing ${doneCount}/${status.total}${pct}${current}`;
    }
    const failedNote = status.failed > 0 ? ` · ${status.failed} failed` : "";
    return `Indexed ${status.succeeded}/${status.total}${failedNote}`;
  }

  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      <span
        className={`h-1.5 w-1.5 shrink-0 rounded-full ${
          isActive ? "animate-pulse bg-amber-500" : status && status.failed > 0 ? "bg-destructive" : "bg-emerald-500"
        }`}
        aria-hidden="true"
      />
      <span title={status?.last_indexed_at ? `Last fully indexed: ${status.last_indexed_at}` : undefined}>
        {summary()}
      </span>
      <button
        type="button"
        onClick={() => void handleRebuild()}
        disabled={isRebuilding || isActive}
        className="rounded-md border px-2 py-1 text-xs hover:bg-accent disabled:opacity-50"
        title="Re-run parsing/OCR/chunking/embeddings for every file in this workspace"
      >
        {isActive ? "Indexing…" : "Rebuild Index"}
      </button>
    </div>
  );
}
