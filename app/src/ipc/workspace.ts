import { ipcInvoke } from "@/ipc/client";
import type { IndexingStatus, Workspace } from "@/ipc/types";

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

/**
 * Live indexing progress for a workspace, read from the `jobs` table
 * (queued/running/succeeded/failed counts + progress percentage). Backend
 * command already existed; this is the first frontend wrapper for it.
 */
export function workspaceIndexingStatus(workspaceId: number): Promise<IndexingStatus> {
  return ipcInvoke<IndexingStatus>("workspace_indexing_status", { workspaceId });
}

/**
 * "Rebuild Workspace Index": re-walks the workspace root and re-enqueues
 * every file for indexing (not fine-tuning/retraining a model -- the
 * architecture contract explicitly rules that out; this rebuilds the
 * existing Parsing -> OCR -> Chunking -> Embeddings -> Vector DB
 * pipeline's output for every file). Returns the number of files
 * enqueued; poll `workspaceIndexingStatus` afterward for progress.
 */
export function workspaceReindex(workspaceId: number): Promise<number> {
  return ipcInvoke<number>("workspace_reindex", { workspaceId });
}
