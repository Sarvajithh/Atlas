import { useEffect, useMemo, useRef, useState } from "react";

import { documentRead } from "@/ipc/document";
import { bookmarkCreate, bookmarkDelete, bookmarkList } from "@/ipc/bookmark";
import type { DocumentContent } from "@/ipc/types";
import { useDocumentStore, type DocumentTab } from "@/state/documents";
import { useAppStore } from "@/state/store";
import { LoadingState, ErrorState } from "@/components/states/StateViews";
import { DocumentOutline } from "@/components/document/DocumentOutline";
import { PdfViewer } from "@/components/document/viewers/PdfViewer";
import { MarkdownViewer, extractHeadings, type HeadingOutlineItem } from "@/components/document/viewers/MarkdownViewer";
import { ImageViewer } from "@/components/document/viewers/ImageViewer";
import { TextViewer, UnsupportedPreview } from "@/components/document/viewers/TextViewer";
import { ResizablePanel } from "@/components/layout/ResizablePanel";

const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3];

/**
 * Document Viewer (§8.2.2-§8.2.4, §44 Viewer Contract). Owns the chrome
 * shared by every file type: Breadcrumb Navigation, Zoom Controls, Search
 * Within Document, Reading Progress, Bookmarks, Document Outline -- and
 * dispatches to the right real renderer for `content.file_type`.
 */
export function DocumentViewer({ tab }: { tab: DocumentTab }) {
  const [content, setContent] = useState<DocumentContent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [searchQuery, setSearchQuery] = useState("");
  // Defaults closed: the outline/page-thumbnail rail is useful on demand
  // (jump to a heading/page) but shouldn't cost permanent width by
  // default -- the document itself is the primary workspace here. The
  // "Outline" button in the toolbar still opens it on demand.
  const [outlineOpen, setOutlineOpen] = useState(false);
  const [progress, setProgress] = useState(0);
  const [pageCount, setPageCount] = useState<number | null>(null);
  const [pageJumpDraft, setPageJumpDraft] = useState("");
  const [pageJumpError, setPageJumpError] = useState(false);
  const [selectedHeadingId, setSelectedHeadingId] = useState<string | null>(null);
  const pdfGoToPage = useRef<((page: number) => void) | null>(null);

  const workspaces = useAppStore((s) => s.workspaces);
  const workspace = workspaces.find((w) => w.id === tab.workspaceId);

  const bookmarks = useDocumentStore((s) => s.bookmarksByDocument[tab.documentId] ?? []);
  const setBookmarksForDocument = useDocumentStore((s) => s.setBookmarksForDocument);
  const addBookmark = useDocumentStore((s) => s.addBookmark);
  const removeBookmark = useDocumentStore((s) => s.removeBookmark);
  const pendingNavigation = useDocumentStore((s) => s.pendingNavigation);
  const clearPendingNavigation = useDocumentStore((s) => s.clearPendingNavigation);

  useEffect(() => {
    let cancelled = false;
    setContent(null);
    setError(null);
    setPageCount(null);
    setZoom(1);
    setSearchQuery("");

    documentRead(tab.documentId)
      .then((c) => !cancelled && setContent(c))
      .catch((err) => !cancelled && setError(err instanceof Error ? err.message : String(err)));

    bookmarkList(tab.documentId)
      .then((list) => !cancelled && setBookmarksForDocument(tab.documentId, list))
      .catch(() => {
        // §45.1 recoverable: bookmarks are supplementary; a failure here
        // shouldn't block the document itself from displaying.
      });

    return () => {
      cancelled = true;
    };
  }, [tab.documentId, setBookmarksForDocument]);

  // §44.2 "Assistant -> Viewer (AI Overlay)": a citation's location_ref
  // (§44.1) arrives here as `pendingNavigation`. `location_ref` is the
  // simple opaque string format already used by bookmarks (`page:N` for
  // PDFs, `start` otherwise) -- for PDFs this jumps to the cited page via
  // the same `pdfGoToPage` ref the outline/search-hit navigation already
  // uses; other formats are consumed (cleared) without a scroll target
  // since no richer Block-based LocationRef (§35.1) is exposed over IPC
  // yet, rather than silently doing nothing forever.
  useEffect(() => {
    if (!pendingNavigation || pendingNavigation.documentId !== tab.documentId || !content) return;
    if (content.file_type === "pdf") {
      const match = /^page:(\d+)$/.exec(pendingNavigation.locationRef);
      if (match) pdfGoToPage.current?.(Number(match[1]));
    }
    clearPendingNavigation();
  }, [pendingNavigation, tab.documentId, content, clearPendingNavigation]);

  const headings: HeadingOutlineItem[] | null = useMemo(() => {
    if (!content || content.file_type !== "md") return null;
    return extractHeadings(content.content);
  }, [content]);

  function currentLocationRef(): string {
    // §35.1 LocationRef is a simple opaque string; for markdown, the
    // current scroll position doesn't map to a stable location without a
    // real block/offset model (§35.1's Block-based LocationRef, which
    // isn't exposed over IPC yet) -- bookmarking here records "top of
    // document" rather than a fabricated precise offset.
    return content?.file_type === "pdf" ? "page:1" : "start";
  }

  async function toggleBookmark() {
    const existing = bookmarks.find((b) => b.location_ref === currentLocationRef());
    if (existing) {
      await bookmarkDelete(existing.id);
      removeBookmark(tab.documentId, existing.id);
      return;
    }
    const created = await bookmarkCreate(tab.documentId, currentLocationRef(), tab.title);
    addBookmark(tab.documentId, created);
  }

  function zoomIn() {
    const next = ZOOM_STEPS.find((z) => z > zoom);
    if (next) setZoom(next);
  }
  function zoomOut() {
    const next = [...ZOOM_STEPS].reverse().find((z) => z < zoom);
    if (next) setZoom(next);
  }

  const isBookmarked = bookmarks.some((b) => b.location_ref === currentLocationRef());

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between border-b px-3 text-xs text-muted-foreground">
        <div className="flex min-w-0 items-center gap-1">
          <span className="truncate">{workspace?.display_name ?? "Workspace"}</span>
          {tab.relativePath.split("/").map((part, i) => (
            <span key={i} className="flex items-center gap-1">
              <span aria-hidden>/</span>
              <span className="truncate text-foreground">{part}</span>
            </span>
          ))}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <button type="button" onClick={() => setOutlineOpen(!outlineOpen)} className="rounded px-1.5 py-0.5 hover:bg-accent">
            Outline
          </button>
          {content?.file_type === "pdf" && pageCount ? (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const target = Number(pageJumpDraft);
                if (Number.isInteger(target) && target >= 1 && target <= pageCount) {
                  pdfGoToPage.current?.(target);
                  setPageJumpError(false);
                } else {
                  setPageJumpError(true);
                }
              }}
              className="flex items-center gap-1"
              aria-label="Jump to page"
            >
              <span aria-hidden>Page</span>
              <input
                value={pageJumpDraft}
                onChange={(e) => {
                  setPageJumpDraft(e.target.value.replace(/[^0-9]/g, ""));
                  setPageJumpError(false);
                }}
                inputMode="numeric"
                aria-label={`Go to page (1-${pageCount})`}
                placeholder="#"
                className={`w-12 rounded-md border bg-background px-1.5 py-0.5 text-center ${pageJumpError ? "border-destructive text-destructive" : ""}`}
              />
              <span aria-hidden className="text-muted-foreground">
                / {pageCount}
              </span>
            </form>
          ) : null}
          <input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search in document…"
            aria-label="Search within document"
            className="w-40 rounded-md border bg-background px-2 py-0.5"
          />
          <button type="button" onClick={zoomOut} aria-label="Zoom out" className="rounded px-1.5 py-0.5 hover:bg-accent">
            −
          </button>
          <span className="w-10 text-center">{Math.round(zoom * 100)}%</span>
          <button type="button" onClick={zoomIn} aria-label="Zoom in" className="rounded px-1.5 py-0.5 hover:bg-accent">
            +
          </button>
          <button
            type="button"
            onClick={() => void toggleBookmark()}
            aria-pressed={isBookmarked}
            aria-label="Toggle bookmark"
            className={isBookmarked ? "rounded px-1.5 py-0.5 text-amber-500" : "rounded px-1.5 py-0.5 hover:bg-accent"}
          >
            {isBookmarked ? "★" : "☆"}
          </button>
        </div>
      </div>

      <div className="relative h-0.5 w-full bg-border">
        <div
          role="progressbar"
          aria-valuenow={Math.round(progress * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
          className="h-full bg-primary transition-[width]"
          style={{ width: `${progress * 100}%` }}
        />
      </div>

      <div className="flex flex-1 overflow-hidden">
        {outlineOpen ? (
          <ResizablePanel
            id="documentViewer.outline"
            defaultWidth={208}
            minWidth={160}
            maxWidth={420}
            handleSide="end"
            handleAriaLabel="Resize document outline"
          >
            <aside aria-label="Document Outline" className="h-full w-full overflow-auto border-r">
              <DocumentOutline
                headings={headings}
                pageCount={content?.file_type === "pdf" ? pageCount : null}
                onSelectHeading={setSelectedHeadingId}
                onSelectPage={(page) => pdfGoToPage.current?.(page)}
              />
            </aside>
          </ResizablePanel>
        ) : null}

        <div className="flex-1 overflow-hidden">
          {error ? (
            <ErrorState message={error} />
          ) : !content ? (
            <LoadingState label="Loading document…" />
          ) : content.file_type === "pdf" ? (
            <PdfViewer
              base64={content.content}
              zoom={zoom}
              searchQuery={searchQuery}
              onPageCount={setPageCount}
              pageRef={pdfGoToPage}
            />
          ) : content.file_type === "md" ? (
            <MarkdownViewer
              content={content.content}
              zoom={zoom}
              searchQuery={searchQuery}
              scrollToHeadingId={selectedHeadingId}
              onProgress={setProgress}
            />
          ) : content.file_type === "image" ? (
            <ImageViewer base64={content.content} mime={content.mime} zoom={zoom} />
          ) : content.mime.startsWith("text/") ? (
            <TextViewer content={content.content} zoom={zoom} searchQuery={searchQuery} />
          ) : (
            <UnsupportedPreview fileType={content.file_type} relativePath={content.relative_path} />
          )}
        </div>
      </div>
    </div>
  );
}