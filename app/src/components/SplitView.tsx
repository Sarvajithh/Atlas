import type { ReactNode } from "react";

import { useAppStore } from "@/state/store";

/**
 * Split View (§8.1). Structural layout only: when enabled, renders the
 * same main-area content twice side by side (a real second independent
 * pane -- e.g. two different documents open side by side -- needs
 * per-pane document state, which depends on the not-yet-existing
 * `document.*` IPC surface; see `Tabs.tsx`/`Sidebar.tsx`).
 */
export function SplitView({ children }: { children: ReactNode }) {
  const isSplitViewOpen = useAppStore((s) => s.isSplitViewOpen);

  if (!isSplitViewOpen) return <>{children}</>;

  return (
    <div className="flex h-full w-full divide-x">
      <div className="w-1/2 overflow-auto">{children}</div>
      <div className="w-1/2 overflow-auto">{children}</div>
    </div>
  );
}
