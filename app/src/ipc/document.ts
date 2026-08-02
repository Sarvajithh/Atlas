import { ipcInvoke } from "@/ipc/client";
import type { DocumentContent, DocumentRecord } from "@/ipc/types";

/**
 * `document.*` namespace (§43.1). Mirrors the new backend `document_*`
 * Tauri commands (`app-tauri/src/commands/document.rs`), which are thin
 * passthroughs to the pre-existing `DocumentRepository`
 * (`atlas-indexer`/`atlas-db`) -- no new business logic.
 */
export function documentList(workspaceId: number): Promise<DocumentRecord[]> {
  return ipcInvoke<DocumentRecord[]>("document_list", { workspaceId });
}

export function documentGet(documentId: number): Promise<DocumentRecord | null> {
  return ipcInvoke<DocumentRecord | null>("document_get", { documentId });
}

export function documentRead(documentId: number): Promise<DocumentContent> {
  return ipcInvoke<DocumentContent>("document_read", { documentId });
}
