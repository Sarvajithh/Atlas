import type { HeadingOutlineItem } from "@/components/document/viewers/MarkdownViewer";

/**
 * Document Outline (§8.2.5). Markdown: real headings extracted from the
 * document's own text (`extractHeadings`, see MarkdownViewer.tsx). PDF:
 * no per-heading outline is resolved in this pass (see PdfViewer.tsx
 * comment) -- a flat "Page N" list is shown instead of fabricating
 * section titles.
 */
export function DocumentOutline({
  headings,
  pageCount,
  onSelectHeading,
  onSelectPage,
}: {
  headings: HeadingOutlineItem[] | null;
  pageCount: number | null;
  onSelectHeading?: (id: string) => void;
  onSelectPage?: (page: number) => void;
}) {
  if (headings) {
    if (headings.length === 0) {
      return <p className="p-3 text-xs text-muted-foreground">No headings found in this document.</p>;
    }
    return (
      <ul className="p-1 text-sm">
        {headings.map((h) => (
          <li key={h.id}>
            <button
              type="button"
              onClick={() => onSelectHeading?.(h.id)}
              style={{ paddingLeft: `${(h.depth - 1) * 12 + 8}px` }}
              className="block w-full truncate py-1 text-left hover:bg-accent"
            >
              {h.text}
            </button>
          </li>
        ))}
      </ul>
    );
  }

  if (pageCount) {
    return (
      <ul className="p-1 text-sm">
        {Array.from({ length: pageCount }, (_, i) => i + 1).map((page) => (
          <li key={page}>
            <button
              type="button"
              onClick={() => onSelectPage?.(page)}
              className="block w-full py-1 pl-2 text-left hover:bg-accent"
            >
              Page {page}
            </button>
          </li>
        ))}
      </ul>
    );
  }

  return <p className="p-3 text-xs text-muted-foreground">No outline available.</p>;
}
