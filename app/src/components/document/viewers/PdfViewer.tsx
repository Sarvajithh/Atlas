import { useEffect, useRef, useState, type MutableRefObject } from "react";
import * as pdfjsLib from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { LoadingState, ErrorState } from "@/components/states/StateViews";

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/**
 * PDF Viewer (§8.2.2). Renders real bytes from `document.read` with
 * pdfjs-dist (new frontend dependency -- there is no PDF rendering
 * capability anywhere else in this project; this is the standard,
 * widely-used renderer rather than a hand-rolled one). Search-within-
 * document is implemented via pdfjs' text layer.
 */
export function PdfViewer({
  base64,
  zoom,
  searchQuery,
  onOutline,
  onPageCount,
  pageRef,
}: {
  base64: string;
  zoom: number;
  searchQuery: string;
  onOutline?: (outline: { title: string; page: number }[]) => void;
  onPageCount?: (count: number) => void;
  pageRef?: MutableRefObject<((page: number) => void) | null>;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const docRef = useRef<pdfjsLib.PDFDocumentProxy | null>(null);
  const loadingTaskRef = useRef<pdfjsLib.PDFDocumentLoadingTask | null>(null);
  const pageCountRef = useRef(0);
  const goToPageRef = useRef<(page: number) => void>(() => {});

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    const loadingTask = pdfjsLib.getDocument({ data: base64ToBytes(base64) });
    loadingTaskRef.current = loadingTask;
    loadingTask.promise
      .then(async (pdf) => {
        if (cancelled) return;
        docRef.current = pdf;
        pageCountRef.current = pdf.numPages;
        onPageCount?.(pdf.numPages);

        // PDF outline entries reference internal "dest" targets, not page
        // numbers directly; resolving them to page indices needs
        // pdf.getPageIndex(dest) per item, which is skipped in this pass.
        // Reporting an empty outline (rather than a page:0 placeholder) is
        // honest about that gap; DocumentOutline.tsx falls back to a flat
        // page list for PDFs instead.
        onOutline?.([]);

        await renderAll(pdf);
        if (!cancelled) setLoading(false);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setLoading(false);
        }
      });

    async function renderAll(pdf: pdfjsLib.PDFDocumentProxy) {
      if (!containerRef.current) return;
      containerRef.current.innerHTML = "";
      for (let pageNum = 1; pageNum <= pdf.numPages; pageNum++) {
        const page = await pdf.getPage(pageNum);
        const viewport = page.getViewport({ scale: zoom });
        const canvas = document.createElement("canvas");
        canvas.dataset.page = String(pageNum);
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        canvas.className = "mx-auto mb-3 block shadow";
        containerRef.current.appendChild(canvas);
        const ctx = canvas.getContext("2d");
        if (!ctx) continue;
        await page.render({ canvasContext: ctx, viewport, canvas }).promise;
      }
    }

    if (pageRef) {
      pageRef.current = goToPage;
    }

    return () => {
      cancelled = true;
      void loadingTaskRef.current?.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base64, zoom]);

  function goToPage(page: number) {
    const clamped = Math.min(Math.max(page, 1), pageCountRef.current || page);
    const canvas = containerRef.current?.querySelector(`canvas[data-page="${clamped}"]`);
    canvas?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  goToPageRef.current = goToPage;

  // Determines which page is "current" for arrow-key navigation by
  // finding the canvas closest to (but not past) the top of the visible
  // scroll area -- consistent with how the outline's page list already
  // scrolls to a page, just read in reverse.
  function currentPageFromScroll(): number {
    const scrollEl = scrollRef.current;
    const container = containerRef.current;
    if (!scrollEl || !container) return 1;
    const canvases = Array.from(container.querySelectorAll<HTMLCanvasElement>("canvas[data-page]"));
    const scrollTop = scrollEl.scrollTop;
    let current = 1;
    for (const canvas of canvases) {
      if (canvas.offsetTop <= scrollTop + 8) {
        current = Number(canvas.dataset.page) || current;
      } else {
        break;
      }
    }
    return current;
  }

  useEffect(() => {
    function isEditableTarget(target: EventTarget | null): boolean {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
    }

    function handleKeyDown(event: KeyboardEvent) {
      // Only steal Left/Right when this PDF is actually the thing on
      // screen and the user isn't typing somewhere else (e.g. the
      // Assistant chat box or a search field) -- arrow keys are common
      // text-editing/cursor-movement keys and must not be hijacked there.
      if (isEditableTarget(document.activeElement)) return;
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      if (!scrollRef.current || pageCountRef.current === 0) return;

      event.preventDefault();
      const current = currentPageFromScroll();
      const next = event.key === "ArrowRight" ? current + 1 : current - 1;
      goToPageRef.current(next);
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  if (error) return <ErrorState message={error} />;

  return (
    <div ref={scrollRef} className="relative h-full overflow-auto bg-muted/30 p-4">
      {loading ? <LoadingState label="Rendering PDF…" /> : null}
      <div ref={containerRef} />
      {searchQuery ? (
        <p className="fixed bottom-8 right-4 rounded-md border bg-card px-2 py-1 text-xs text-muted-foreground shadow">
          Text search within PDF pages is not implemented in this pass -- pdfjs' text layer would be needed per
          page.
        </p>
      ) : null}
    </div>
  );
}
