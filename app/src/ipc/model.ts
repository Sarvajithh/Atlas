import { ipcInvoke } from "@/ipc/client";
import type { EngineRole, ModelRegistryEntry } from "@/ipc/types";

/** `model.*` namespace (§43.1, V1.0 Part 3). Mirrors backend `model_list`. */
export function modelList(): Promise<ModelRegistryEntry[]> {
  return ipcInvoke<ModelRegistryEntry[]>("model_list");
}

/**
 * Manually select `modelIdentifier` as the model used for `role`. Mirrors
 * backend `model_select`, which is the only place a role's active model
 * ever changes -- selection is entirely manual, never automatic. Rejects
 * (with a `ModelError` message) any model that wasn't discovered as
 * compatible with `role`.
 */
export function modelSelect(role: EngineRole, modelIdentifier: string): Promise<ModelRegistryEntry> {
  return ipcInvoke<ModelRegistryEntry>("model_select", { role, modelIdentifier });
}
