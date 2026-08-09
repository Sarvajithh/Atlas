/**
 * Model Dashboard (V1.0 Part 3): shows every Ollama model discovered on
 * this machine, grouped by which Engine role(s) (§14.1) each is
 * compatible with, and lets the user MANUALLY select which single model
 * backs each role.
 *
 * Selection is entirely manual and explicit:
 * - Discovery (`ModelDiscoveryService::run`, backend) never auto-selects a
 *   model for a role -- it only ever lists candidates and preserves a
 *   selection the user already made.
 * - Runtime engines never fall back to a different model on failure.
 * - The ONLY way a role's active model changes is the user picking a
 *   radio button here, which calls the `model_select` IPC command.
 * - A role with no selection shows a clear "No model selected" state
 *   instead of silently working with something the user didn't choose.
 *
 * Backed by `model.list` / `model.select` IPC commands
 * (`AppFacade::model_registry()`, populated by `ModelDiscoveryService`
 * from live Ollama discovery -- see `atlas-models::discovery`).
 *
 * `vram_requirement` is `None` for every entry `ModelDiscoveryService`
 * currently writes (it's never populated during discovery) -- shown
 * honestly as "Unknown" rather than a fabricated estimate.
 */
import { useEffect, useState } from "react";

import { modelList, modelSelect } from "@/ipc/model";
import type { EngineRole, ModelRegistryEntry, ModelStatus } from "@/ipc/types";
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

// Roles the Model Dashboard shows a selector for, in a stable, sensible
// order (matches §14.1's Engine table order). A role with zero discovered
// candidates is simply omitted from the grouped list below -- there is
// nothing to select from yet.
const ROLE_ORDER: EngineRole[] = [
  "Tutor",
  "Reasoning",
  "Planner",
  "Embedding",
  "Retriever",
  "Reranker",
  "Vision",
  "Ocr",
  "Memory",
  "Analytics",
];

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
  // Per-role selection-in-flight state, so one role's pending selection
  // doesn't disable the radio buttons for every other role.
  const [pendingRole, setPendingRole] = useState<EngineRole | null>(null);
  const [selectError, setSelectError] = useState<string | null>(null);

  const load = () => {
    setIsLoading(true);
    setError(null);
    modelList()
      .then(setEntries)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setIsLoading(false));
  };

  useEffect(load, []);

  const handleSelect = async (role: EngineRole, modelIdentifier: string) => {
    setSelectError(null);
    setPendingRole(role);
    try {
      await modelSelect(role, modelIdentifier);
      // Re-fetch so the whole registry (including any other row that got
      // unselected server-side) reflects the real, persisted state,
      // rather than optimistically guessing it client-side.
      const fresh = await modelList();
      setEntries(fresh);
    } catch (err) {
      setSelectError(err instanceof Error ? err.message : String(err));
    } finally {
      setPendingRole(null);
    }
  };

  // Group by role so the dashboard reads as "one row per Engine role",
  // matching how §14.1 roles are actually consumed (`ModelProvider::current_model_for`),
  // even though the underlying table is one row per (model, role) pair.
  const byRole = new Map<string, ModelRegistryEntry[]>();
  for (const entry of entries ?? []) {
    const list = byRole.get(entry.engine_role) ?? [];
    list.push(entry);
    byRole.set(entry.engine_role, list);
  }
  const orderedRoles = ROLE_ORDER.filter((role) => byRole.has(role));

  return (
    <section aria-label="Model Dashboard" className="flex h-full flex-col gap-4 overflow-auto p-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold">Model Dashboard</h1>
          <p className="text-sm text-muted-foreground">
            Choose which discovered Ollama model backs each Engine role. Atlas never picks or switches a model
            automatically -- your selection here is what every request for that role uses.
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

      {selectError ? (
        <div role="alert" className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Couldn't change model: {selectError}
        </div>
      ) : null}

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
          {orderedRoles.map((role) => {
            const roleEntries = byRole.get(role) ?? [];
            const hasSelection = roleEntries.some((e) => e.is_selected_for_role);
            const groupName = `model-role-${role}`;
            return (
              <div key={role} className="rounded-lg border p-3">
                <div className="mb-2 flex items-center justify-between">
                  <h2 className="text-sm font-semibold">{role}</h2>
                  {!hasSelection ? (
                    <span className="rounded bg-destructive/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-destructive">
                      No model selected
                    </span>
                  ) : null}
                </div>
                <div role="radiogroup" aria-label={`Select model for ${role}`} className="flex flex-col divide-y">
                  {roleEntries.map((entry) => {
                    const inputId = `${groupName}-${entry.model_identifier}`;
                    const disabled = pendingRole === role || entry.status === "Unavailable";
                    return (
                      <label
                        htmlFor={inputId}
                        key={`${entry.engine_role}-${entry.model_identifier}`}
                        className={`flex flex-wrap items-center justify-between gap-2 py-2 ${disabled ? "opacity-60" : "cursor-pointer"}`}
                      >
                        <div className="flex items-center gap-3">
                          <input
                            id={inputId}
                            type="radio"
                            name={groupName}
                            checked={entry.is_selected_for_role}
                            disabled={disabled}
                            onChange={() => handleSelect(role, entry.model_identifier)}
                            className="h-4 w-4"
                          />
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
                      </label>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
