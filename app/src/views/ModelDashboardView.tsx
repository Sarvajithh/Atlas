/**
 * Model Dashboard (V1.0 Part 3): read-only view of the real
 * `model_registry` table -- which Ollama models are discovered, which
 * role(s) each is assigned to (§14.1 Engine roles), its loaded/available
 * status, context window size, and VRAM requirement where known.
 *
 * Backed by the real `model.list` IPC command (`AppFacade::model_registry()`,
 * populated by `ModelDiscoveryService` from live Ollama discovery -- see
 * `atlas-models::discovery`). No write/assign UI here: reassigning a role
 * to a different model needs real conflict-resolution design (a role can
 * only have one active selection) that's out of scope for this pass, so
 * this view is intentionally read-only, per spec ("Read-only is
 * acceptable").
 *
 * `vram_requirement` is `None` for every entry `ModelDiscoveryService`
 * currently writes (it's never populated during discovery) -- shown
 * honestly as "Unknown" rather than a fabricated estimate.
 */
import { useEffect, useState } from "react";

import { modelList } from "@/ipc/model";
import type { ModelRegistryEntry, ModelStatus } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

const STATUS_LABEL: Record<ModelStatus, string> = {
  Available: "Available",
  Loading: "Loading",
  Unavailable: "Unavailable",
  Error: "Error",
};

const STATUS_DOT: Record<ModelStatus, string> = {
  Available: "bg-green-500",
  Loading: "bg-amber-500",
  Unavailable: "bg-muted-foreground",
  Error: "bg-destructive",
};

function formatContextLength(tokens: number): string {
  if (tokens <= 0) return "Unknown";
  if (tokens >= 1000) return `${(tokens / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 })}K tokens`;
  return `${tokens} tokens`;
}

function formatVram(bytes: number | null): string {
  if (bytes === null || bytes <= 0) return "Unknown";
  const gb = bytes / 1024 ** 3;
  return `${gb.toLocaleString(undefined, { maximumFractionDigits: 1 })} GB`;
}

export function ModelDashboardView() {
  const [entries, setEntries] = useState<ModelRegistryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const load = () => {
    setIsLoading(true);
    setError(null);
    modelList()
      .then(setEntries)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setIsLoading(false));
  };

  useEffect(load, []);

  // Group by role so the dashboard reads as "one row per Engine role",
  // matching how §14.1 roles are actually consumed (`ModelProvider::current_model_for`),
  // even though the underlying table is one row per (model, role) pair.
  const byRole = new Map<string, ModelRegistryEntry[]>();
  for (const entry of entries ?? []) {
    const list = byRole.get(entry.engine_role) ?? [];
    list.push(entry);
    byRole.set(entry.engine_role, list);
  }

  return (
    <section aria-label="Model Dashboard" className="flex h-full flex-col gap-4 overflow-auto p-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold">Model Dashboard</h1>
          <p className="text-sm text-muted-foreground">
            Ollama models discovered on this machine and the role(s) each is assigned to.
          </p>
        </div>
        <button
          type="button"
          onClick={load}
          disabled={isLoading}
          className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-50"
        >
          {isLoading ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {error ? (
        <ErrorState message={error} onRetry={load} />
      ) : entries === null ? (
        <LoadingState label="Loading model registry…" />
      ) : entries.length === 0 ? (
        <EmptyState
          title="No models discovered yet"
          description="No models have been discovered from Ollama. Make sure Ollama is running with at least one model installed, then restart Atlas or re-run discovery."
        />
      ) : (
        <div className="flex flex-col gap-4">
          {Array.from(byRole.entries()).map(([role, roleEntries]) => (
            <div key={role} className="rounded-lg border p-3">
              <h2 className="mb-2 text-sm font-semibold">{role}</h2>
              <div className="flex flex-col divide-y">
                {roleEntries.map((entry) => (
                  <div
                    key={`${entry.engine_role}-${entry.model_identifier}`}
                    className="flex flex-wrap items-center justify-between gap-2 py-2"
                  >
                    <div className="flex items-center gap-2">
                      <span className={`h-2 w-2 shrink-0 rounded-full ${STATUS_DOT[entry.status]}`} />
                      <div>
                        <p className="text-sm font-medium">
                          {entry.model_identifier}
                          {entry.is_selected_for_role ? (
                            <span className="ml-2 rounded bg-accent px-1.5 py-0.5 text-[10px] font-normal uppercase tracking-wide text-muted-foreground">
                              Active
                            </span>
                          ) : null}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {STATUS_LABEL[entry.status]} · version {entry.version}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-4 text-xs text-muted-foreground">
                      <span>Context: {formatContextLength(entry.context_length)}</span>
                      <span>VRAM: {formatVram(entry.vram_requirement)}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
