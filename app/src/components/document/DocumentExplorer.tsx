import { useEffect, useMemo, useState } from "react";

import { documentList } from "@/ipc/document";
import type { DocumentRecord } from "@/ipc/types";
import { EmptyState, ErrorState, LoadingState } from "@/components/states/StateViews";

interface TreeFolder {
  name: string;
  path: string;
  folders: Map<string, TreeFolder>;
  documents: DocumentRecord[];
}

function buildTree(documents: DocumentRecord[]): TreeFolder {
  const root: TreeFolder = { name: "", path: "", folders: new Map(), documents: [] };
  for (const doc of documents) {
    const parts = doc.relative_path.split("/");
    const fileName = parts.pop();
    if (!fileName) continue;
    let cursor = root;
    let pathSoFar = "";
    for (const part of parts) {
      pathSoFar = pathSoFar ? `${pathSoFar}/${part}` : part;
      let next = cursor.folders.get(part);
      if (!next) {
        next = { name: part, path: pathSoFar, folders: new Map(), documents: [] };
        cursor.folders.set(part, next);
      }
      cursor = next;
    }
    cursor.documents.push(doc);
  }
  return root;
}

const FILE_ICON: Record<string, string> = {
  md: "📝",
  pdf: "📕",
  docx: "📘",
  image: "🖼",
};

function Folder({ folder, depth, onOpen }: { folder: TreeFolder; depth: number; onOpen: (d: DocumentRecord) => void }) {
  const [expanded, setExpanded] = useState(depth < 1);
  const childFolders = Array.from(folder.folders.values()).sort((a, b) => a.name.localeCompare(b.name));
  const files = [...folder.documents].sort((a, b) => a.relative_path.localeCompare(b.relative_path));

  return (
    <div>
      {folder.name ? (
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          className="flex w-full items-center gap-1.5 py-1 text-left text-sm hover:bg-accent"
        >
          <span aria-hidden className="text-xs text-muted-foreground">
            {expanded ? "▾" : "▸"}
          </span>
          <span className="truncate">{folder.name}</span>
        </button>
      ) : null}
      {expanded ? (
        <div>
          {childFolders.map((child) => (
            <Folder key={child.path} folder={child} depth={depth + 1} onOpen={onOpen} />
          ))}
          {files.map((doc) => {
            const name = doc.relative_path.split("/").pop();
            return (
              <button
                key={doc.id}
                type="button"
                onClick={() => onOpen(doc)}
                style={{ paddingLeft: `${(depth + 1) * 12 + 8}px` }}
                className="flex w-full items-center gap-1.5 py-1 text-left text-sm hover:bg-accent"
                title={doc.parse_status === "Failed" ? "Indexing failed for this file" : undefined}
              >
                <span aria-hidden>{FILE_ICON[doc.file_type] ?? "📄"}</span>
                <span className="truncate">{name}</span>
                {doc.parse_status === "Failed" ? (
                  <span aria-hidden className="ml-auto text-destructive">
                    !
                  </span>
                ) : doc.parse_status !== "Parsed" ? (
                  <span aria-hidden className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500" />
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Document Explorer / Folder Tree (§8.1, §8.2.2). Built from real
 * `document.list` results (§43.1), grouped into folders client-side from
 * `relative_path` -- the backend stores documents as a flat table keyed by
 * relative path (§33.2), so tree structure here is presentation-only
 * derivation, not new business logic.
 */
export function DocumentExplorer({
  workspaceId,
  onOpenDocument,
}: {
  workspaceId: number;
  onOpenDocument: (doc: DocumentRecord) => void;
}) {
  const [documents, setDocuments] = useState<DocumentRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  async function load() {
    setError(null);
    setDocuments(null);
    try {
      const list = await documentList(workspaceId);
      setDocuments(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  const filtered = useMemo(() => {
    if (!documents) return [];
    if (!filter.trim()) return documents;
    const q = filter.toLowerCase();
    return documents.filter((d) => d.relative_path.toLowerCase().includes(q));
  }, [documents, filter]);

  const tree = useMemo(() => buildTree(filtered), [filtered]);

  if (error) {
    return <ErrorState message={error} onRetry={load} />;
  }
  if (documents === null) {
    return <LoadingState label="Loading documents…" />;
  }
  if (documents.length === 0) {
    return (
      <EmptyState
        title="No documents indexed yet"
        description="Files added to this workspace's folder will appear here once indexing finishes."
      />
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b p-2">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter files…"
          aria-label="Filter files"
          className="w-full rounded-md border bg-background px-2 py-1 text-xs"
        />
      </div>
      <div className="flex-1 overflow-auto py-1">
        {filtered.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">No files match “{filter}”.</p>
        ) : (
          <Folder folder={tree} depth={0} onOpen={onOpenDocument} />
        )}
      </div>
    </div>
  );
}
