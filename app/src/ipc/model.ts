import { ipcInvoke } from "@/ipc/client";
import type { ModelRegistryEntry } from "@/ipc/types";

/** `model.*` namespace (§43.1, V1.0 Part 3). Mirrors backend `model_list`. */
export function modelList(): Promise<ModelRegistryEntry[]> {
  return ipcInvoke<ModelRegistryEntry[]>("model_list");
}
