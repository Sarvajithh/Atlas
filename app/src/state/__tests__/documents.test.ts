import { describe, expect, it, beforeEach } from "vitest";

import { useDocumentStore } from "@/state/documents";
import type { DocumentRecord } from "@/ipc/types";

function makeDoc(overrides: Partial<DocumentRecord> = {}): DocumentRecord {
  return {
    id: 1,
    workspace_id: 1,
    relative_path: "notes/chapter1.md",
    content_hash: "hash",
    file_type: "md",
    size: 100,
    mtime: "2026-01-01T00:00:00Z",
    parse_status: "Parsed",
    last_indexed_hash: null,
    authored_at: null,
    ...overrides,
  };
}

describe("useDocumentStore", () => {
  beforeEach(() => {
    useDocumentStore.setState({
      openTabs: [],
      activeTabId: null,
      recentByWorkspace: {},
      bookmarksByDocument: {},
      pendingNavigation: null,
    });
  });

  it("navigateToLocation sets pendingNavigation, clearPendingNavigation resets it", () => {
    useDocumentStore.getState().navigateToLocation(1, "page:4");
    expect(useDocumentStore.getState().pendingNavigation).toEqual({ documentId: 1, locationRef: "page:4" });
    useDocumentStore.getState().clearPendingNavigation();
    expect(useDocumentStore.getState().pendingNavigation).toBeNull();
  });

  it("opens a document as a new tab and makes it active", () => {
    useDocumentStore.getState().openDocument(1, makeDoc());
    const state = useDocumentStore.getState();
    expect(state.openTabs).toHaveLength(1);
    expect(state.openTabs[0].title).toBe("chapter1.md");
    expect(state.activeTabId).toBe("doc:1");
  });

  it("opening the same document twice does not duplicate the tab", () => {
    useDocumentStore.getState().openDocument(1, makeDoc());
    useDocumentStore.getState().openDocument(1, makeDoc());
    expect(useDocumentStore.getState().openTabs).toHaveLength(1);
  });

  it("tracks recently opened documents per workspace, most recent first", () => {
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 1 }));
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 2 }));
    expect(useDocumentStore.getState().recentByWorkspace[1]).toEqual([2, 1]);
  });

  it("re-opening an already-recent document moves it to the front instead of duplicating", () => {
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 1 }));
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 2 }));
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 1 }));
    expect(useDocumentStore.getState().recentByWorkspace[1]).toEqual([1, 2]);
  });

  it("closing the active tab falls back to the previous tab", () => {
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 1, relative_path: "a.md" }));
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 2, relative_path: "b.md" }));
    useDocumentStore.getState().closeDocumentTab("doc:2");
    expect(useDocumentStore.getState().activeTabId).toBe("doc:1");
  });

  it("reorders tabs by index", () => {
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 1, relative_path: "a.md" }));
    useDocumentStore.getState().openDocument(1, makeDoc({ id: 2, relative_path: "b.md" }));
    useDocumentStore.getState().reorderTabs(0, 1);
    expect(useDocumentStore.getState().openTabs.map((t) => t.documentId)).toEqual([2, 1]);
  });

  it("adds and removes bookmarks for a document", () => {
    useDocumentStore.getState().addBookmark(1, {
      id: 1,
      document_id: 1,
      location_ref: "start",
      label: "chapter1.md",
      created_at: "2026-01-01T00:00:00Z",
    });
    expect(useDocumentStore.getState().bookmarksByDocument[1]).toHaveLength(1);
    useDocumentStore.getState().removeBookmark(1, 1);
    expect(useDocumentStore.getState().bookmarksByDocument[1]).toHaveLength(0);
  });
});
