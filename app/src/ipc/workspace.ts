import { ipcInvoke } from "@/ipc/client";
import type { Workspace } from "@/ipc/types";

/** `workspace.*` namespace (§43.1). Mirrors backend `workspace_list`. */
export function workspaceList(): Promise<Workspace[]> {
  return ipcInvoke<Workspace[]>("workspace_list");
}
