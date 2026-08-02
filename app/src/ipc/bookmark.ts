import { ipcInvoke } from "@/ipc/client";
import type { Bookmark } from "@/ipc/types";

/**
 * `bookmark.*` namespace (§43.1). Mirrors the new backend `bookmark_*`
 * Tauri commands, thin passthroughs to the pre-existing
 * `BookmarkRepository` (§33.9, `atlas-memory`/`atlas-db`).
 */
export function bookmarkList(documentId: number): Promise<Bookmark[]> {
  return ipcInvoke<Bookmark[]>("bookmark_list", { documentId });
}

export function bookmarkCreate(documentId: number, locationRef: string, label: string): Promise<Bookmark> {
  return ipcInvoke<Bookmark>("bookmark_create", { documentId, locationRef, label });
}

export function bookmarkDelete(bookmarkId: number): Promise<void> {
  return ipcInvoke<void>("bookmark_delete", { bookmarkId });
}
