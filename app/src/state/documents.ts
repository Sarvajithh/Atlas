import { create } from "zustand";

import type { Bookmark, DocumentRecord } from "@/ipc/types";

/**
 * Document Experience state (§8.2.2-§8.2.4). Kept as its own store
 * (matching the `state/theme.ts` precedent) rather than growing the
 * cross-cutting shell store further. Still a cache of backend truth:
 * `DocumentRecord`s and `Bookmark`s here always originate from
 * `document.*`/`bookmark.*` IPC results.
 *
 * "Recent documents" has no backing backend entity (no `recent_documents`
 * table/repository exists) -- it is session-only, in-memory, and reset on
 * restart. This is disclosed rather than faked as persisted.
 */
export interface DocumentTab {
  tabId: string;
  documentId: number;
  workspaceId: number;
  relativePath: string;
  fileType: string;
  title: string;
}

export interface DocumentState {
  openTabs: DocumentTab[];
  activeTabId: string | null;
  recentByWorkspace: Record<number, number[]>;
  bookmarksByDocument: Record<number, Bookmark[]>;

  openDocument: (workspaceId: number, doc: DocumentRecord) => void;
  closeDocumentTab: (tabId: string) => void;
  setActiveDocumentTab: (tabId: string) => void;
  reorderTabs: (fromIndex: number, toIndex: number) => void;

  setBookmarksForDocument: (documentId: number, bookmarks: Bookmark[]) => void;
  addBookmark: (documentId: number, bookmark: Bookmark) => void;
  removeBookmark: (documentId: number, bookmarkId: number) => void;
}

function titleFor(relativePath: string): string {
  const parts = relativePath.split("/");
  return parts[parts.length - 1] ?? relativePath;
}

export const useDocumentStore = create<DocumentState>((set, get) => ({
  openTabs: [],
  activeTabId: null,
  recentByWorkspace: {},
  bookmarksByDocument: {},

  openDocument: (workspaceId, doc) => {
    const tabId = `doc:${doc.id}`;
    const exists = get().openTabs.some((t) => t.tabId === tabId);
    set((state) => ({
      openTabs: exists
        ? state.openTabs
        : [
            ...state.openTabs,
            {
              tabId,
              documentId: doc.id,
              workspaceId,
              relativePath: doc.relative_path,
              fileType: doc.file_type,
              title: titleFor(doc.relative_path),
            },
          ],
      activeTabId: tabId,
      recentByWorkspace: {
        ...state.recentByWorkspace,
        [workspaceId]: [
          doc.id,
          ...(state.recentByWorkspace[workspaceId] ?? []).filter((id) => id !== doc.id),
        ].slice(0, 10),
      },
    }));
  },

  closeDocumentTab: (tabId) =>
    set((state) => {
      const remaining = state.openTabs.filter((t) => t.tabId !== tabId);
      const wasActive = state.activeTabId === tabId;
      return {
        openTabs: remaining,
        activeTabId: wasActive ? (remaining[remaining.length - 1]?.tabId ?? null) : state.activeTabId,
      };
    }),

  setActiveDocumentTab: (tabId) => set({ activeTabId: tabId }),

  reorderTabs: (fromIndex, toIndex) =>
    set((state) => {
      const tabs = [...state.openTabs];
      const [moved] = tabs.splice(fromIndex, 1);
      if (!moved) return state;
      tabs.splice(toIndex, 0, moved);
      return { openTabs: tabs };
    }),

  setBookmarksForDocument: (documentId, bookmarks) =>
    set((state) => ({
      bookmarksByDocument: { ...state.bookmarksByDocument, [documentId]: bookmarks },
    })),

  addBookmark: (documentId, bookmark) =>
    set((state) => ({
      bookmarksByDocument: {
        ...state.bookmarksByDocument,
        [documentId]: [...(state.bookmarksByDocument[documentId] ?? []), bookmark],
      },
    })),

  removeBookmark: (documentId, bookmarkId) =>
    set((state) => ({
      bookmarksByDocument: {
        ...state.bookmarksByDocument,
        [documentId]: (state.bookmarksByDocument[documentId] ?? []).filter((b) => b.id !== bookmarkId),
      },
    })),
}));
