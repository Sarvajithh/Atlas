import { ipcInvoke } from "@/ipc/client";
import type { Workspace } from "@/ipc/types";

/**
 * `workspace.*` namespace (§43.1). Thin typed wrappers over the backend
 * `workspace_*` Tauri commands (`app-tauri/src/commands/workspace.rs`),
 * which are already fully implemented against `AppFacade`/`WorkspaceEngine`
 * (§6, §21). No new IPC commands are introduced here -- every function
 * below mirrors a handler that already exists.
 */
export function workspaceList(): Promise<Workspace[]> {
  return ipcInvoke<Workspace[]>("workspace_list");
}

export function workspaceGet(workspaceId: number): Promise<Workspace | null> {
  return ipcInvoke<Workspace | null>("workspace_get", { workspaceId });
}

/** §6: "Users never upload files. They link folders." */
export function workspaceLink(rootPath: string, displayName: string): Promise<Workspace> {
  return ipcInvoke<Workspace>("workspace_link", { rootPath, displayName });
}

export function workspaceRename(workspaceId: number, displayName: string): Promise<Workspace> {
  return ipcInvoke<Workspace>("workspace_rename", { workspaceId, displayName });
}

/** §6.1: watching stops; derived data is retained and queryable. */
export function workspaceArchive(workspaceId: number): Promise<Workspace> {
  return ipcInvoke<Workspace>("workspace_archive", { workspaceId });
}

export function workspaceRestore(workspaceId: number): Promise<Workspace> {
  return ipcInvoke<Workspace>("workspace_restore", { workspaceId });
}

/** §6.1: removes the workspace row + watcher registration only; does not delete derived knowledge. */
export function workspaceUnlink(workspaceId: number): Promise<void> {
  return ipcInvoke<void>("workspace_unlink", { workspaceId });
}
