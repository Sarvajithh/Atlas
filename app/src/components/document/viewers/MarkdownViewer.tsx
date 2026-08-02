import { useEffect, useMemo, useRef } from "react";
import ReactMarkdown from "react-markdown";

export interface HeadingOutlineItem {
  id: string;
  depth: number;
  text: string;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, "")
    .replace(/\s+/g, "-");
}

/** Extracts `#`-style headings for the Document Outline (§8.2.5). */
export function extractHeadings(markdown: string): HeadingOutlineItem[] {
  const lines = markdown.split("\n");
  const headings: HeadingOutlineItem[] = [];
  const seen = new Map<string, number>();
  for (const line of lines) {
    const match = /^(#{1,6})\s+(.+)$/.exec(line.trim());
    if (!match) continue;
    const depth = match[1].length;
    const text = match[2].trim();
    let id = slugify(text);
    const count = seen.get(id) ?? 0;
    seen.set(id, count + 1);
    if (count > 0) id = `${id}-${count}`;
    headings.push({ id, depth, text });
  }
  return headings;
}

/**
 * Markdown Viewer (§8.2.3). Renders real text from `document.read` with
 * `react-markdown` (new frontend dependency -- the standard, widely-used
 * renderer; nothing in this project parses/renders Markdown to HTML yet).
 * Backend §36's Markdown parser produces `Block`s for RAG/chunking, a
 * separate concern (§35.1) not reused here -- this is presentation, not
 * duplicated business logic.
 */
export function MarkdownViewer({
  content,
  zoom,
  searchQuery,
  scrollToHeadingId,
  onProgress,
}: {
  content: string;
  zoom: number;
  searchQuery: string;
  scrollToHeadingId?: string | null;
  onProgress?: (fraction: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!scrollToHeadingId) return;
    const el = containerRef.current?.querySelector(`#${CSS.escape(scrollToHeadingId)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [scrollToHeadingId]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el || !onProgress) return;
    const reportProgress = onProgress;
    function handleScroll() {
      if (!el) return;
      const max = el.scrollHeight - el.clientHeight;
      reportProgress(max > 0 ? Math.min(1, el.scrollTop / max) : 1);
    }
    el.addEventListener("scroll", handleScroll);
    return () => el.removeEventListener("scroll", handleScroll);
  }, [onProgress]);

  const highlighted = useMemo(() => {
    if (!searchQuery.trim()) return content;
    // Simple case-insensitive wrap in markdown emphasis for search-within-
    // document (§8.2.8). This is a basic implementation: it can produce
    // slightly malformed markdown if a match falls inside an existing
    // `**bold**` span or a code block, since it operates on raw text
    // rather than the parsed AST. A robust version would highlight after
    // parsing (walking react-markdown's AST/rendered text nodes), which is
    // a larger change than this pass covers.
    const escaped = searchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return content.replace(new RegExp(`(${escaped})`, "gi"), "**$1**");
  }, [content, searchQuery]);

  return (
    <div
      ref={containerRef}
      className="h-full overflow-auto p-8"
      style={{ fontSize: `${zoom * 100}%` }}
    >
      <article className="prose prose-sm mx-auto max-w-3xl dark:prose-invert">
        <ReactMarkdown
          components={{
            h1: (props) => <h1 id={slugify(String(props.children))} {...props} />,
            h2: (props) => <h2 id={slugify(String(props.children))} {...props} />,
            h3: (props) => <h3 id={slugify(String(props.children))} {...props} />,
            h4: (props) => <h4 id={slugify(String(props.children))} {...props} />,
            h5: (props) => <h5 id={slugify(String(props.children))} {...props} />,
            h6: (props) => <h6 id={slugify(String(props.children))} {...props} />,
          }}
        >
          {highlighted}
        </ReactMarkdown>
      </article>
    </div>
  );
}
