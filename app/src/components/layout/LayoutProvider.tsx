import { useEffect, type ReactNode } from "react";

import { useLayoutStore } from "@/components/layout/layoutStore";

/**
 * Global Resizable Panel System -- mounts once near the app root.
 *
 * Hydrates persisted panel widths from the backend on startup (mirrors
 * `hydrateTheme()` in App.tsx). Renders children immediately rather than
 * blocking on load: every `ResizablePanel` already falls back to its own
 * `defaultWidth` until the store is hydrated, so there is nothing to
 * gate -- panels just snap to their persisted width once it arrives,
 * same as the theme flash-of-default-then-persisted pattern already
 * accepted elsewhere in this app.
 */
export function LayoutProvider({ children }: { children: ReactNode }) {
  const hydrate = useLayoutStore((s) => s.hydrate);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  return <>{children}</>;
}
