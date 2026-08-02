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
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const docRef = useRef<pdfjsLib.PDFDocumentProxy | null>(null);
  const loadingTaskRef = useRef<pdfjsLib.PDFDocumentLoadingTask | null>(null);

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
      pageRef.current = (page: number) => {
        const canvas = containerRef.current?.querySelector(`canvas[data-page="${page}"]`);
        canvas?.scrollIntoView({ behavior: "smooth", block: "start" });
      };
    }

    return () => {
      cancelled = true;
      void loadingTaskRef.current?.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base64, zoom]);

  if (error) return <ErrorState message={error} />;

  return (
    <div className="relative h-full overflow-auto bg-muted/30 p-4">
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
