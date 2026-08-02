/**
 * Text/Code Viewer fallback (§8.2). Renders any UTF-8 text `document.read`
 * result in a monospace block. Note: as of this milestone the backend's
 * Parser Selector only registers parsers for `md`/`pdf`/`docx`/`image`
 * (`atlas-indexer/src/pipeline.rs::normalize_file_type`) -- a `.py`/`.txt`
 * file currently fails to index at all and never appears in
 * `document.list`, so this viewer has no real data to render against yet.
 * It exists so that if/when a plain-text file type is added to the Parser
 * Selector, the viewer layer already supports it without further UI work.
 */
export function TextViewer({ content, zoom, searchQuery }: { content: string; zoom: number; searchQuery: string }) {
  return (
    <div className="h-full overflow-auto bg-muted/20 p-4" style={{ fontSize: `${zoom * 100}%` }}>
      <pre className="whitespace-pre-wrap break-words rounded-md border bg-card p-4 font-mono text-sm">
        {searchQuery.trim() ? highlightPlain(content, searchQuery) : content}
      </pre>
    </div>
  );
}

function highlightPlain(content: string, query: string) {
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = content.split(new RegExp(`(${escaped})`, "gi"));
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i}>{part}</mark>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}

/** Honest "no preview available" state for formats without a viewer (e.g. docx). */
export function UnsupportedPreview({ fileType, relativePath }: { fileType: string; relativePath: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center text-sm text-muted-foreground">
      <p className="font-medium text-foreground">No preview available for “{relativePath}”</p>
      <p className="max-w-sm">
        This milestone doesn't include a renderer for “{fileType}” files. The file is indexed and searchable, but
        not previewable here yet.
      </p>
    </div>
  );
}
