import { useState } from "react";

import { useDocumentStore } from "@/state/documents";
import { cn } from "@/state/utils";

const FILE_ICON: Record<string, string> = {
  md: "📝",
  pdf: "📕",
  docx: "📘",
  image: "🖼",
};

/**
 * Multi-tab Documents (§8.2.2). VS Code-style: click to switch, × to
 * close, drag to reorder (Drag & Drop Support). This is separate from the
 * outer `Tabs.tsx` (workspace-level navigation) -- these tabs are scoped
 * to documents open within one workspace's detail view.
 */
export function DocumentTabs() {
  const openTabs = useDocumentStore((s) => s.openTabs);
  const activeTabId = useDocumentStore((s) => s.activeTabId);
  const setActiveDocumentTab = useDocumentStore((s) => s.setActiveDocumentTab);
  const closeDocumentTab = useDocumentStore((s) => s.closeDocumentTab);
  const reorderTabs = useDocumentStore((s) => s.reorderTabs);
  const [dragIndex, setDragIndex] = useState<number | null>(null);

  if (openTabs.length === 0) return null;

  return (
    <div role="tablist" aria-label="Open documents" className="flex h-9 shrink-0 items-stretch overflow-x-auto border-b text-sm">
      {openTabs.map((tab, index) => (
        <div
          key={tab.tabId}
          role="tab"
          aria-selected={tab.tabId === activeTabId}
          draggable
          onDragStart={() => setDragIndex(index)}
          onDragOver={(e) => e.preventDefault()}
          onDrop={() => {
            if (dragIndex !== null && dragIndex !== index) reorderTabs(dragIndex, index);
            setDragIndex(null);
          }}
          className={cn(
            "flex shrink-0 items-center gap-1.5 border-r px-3",
            tab.tabId === activeTabId
              ? "bg-accent text-foreground"
              : "text-muted-foreground hover:bg-accent/60",
          )}
        >
          <span aria-hidden>{FILE_ICON[tab.fileType] ?? "📄"}</span>
          <button
            type="button"
            onClick={() => setActiveDocumentTab(tab.tabId)}
            className="max-w-[10rem] truncate"
            title={tab.relativePath}
          >
            {tab.title}
          </button>
          <button
            type="button"
            aria-label={`Close ${tab.title}`}
            onClick={() => closeDocumentTab(tab.tabId)}
            className="text-muted-foreground hover:text-foreground"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
