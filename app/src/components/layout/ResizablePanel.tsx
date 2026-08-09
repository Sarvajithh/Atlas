import { useCallback, useMemo, type ReactNode } from "react";

import { cn } from "@/state/utils";
import { useLayoutStore } from "@/components/layout/layoutStore";
import { ResizeHandle } from "@/components/layout/ResizeHandle";

/**
 * Global Resizable Panel System (V1.0 Part 1).
 *
 * A fixed-width panel with a single draggable edge, replacing the
 * `w-64`/`w-72`-style hardcoded widths previously used for the workspace
 * sidebar, the Assistant panel, and the Document Workspace explorer.
 *
 * Width persistence is keyed by `id` through `layoutStore`, which mirrors
 * the value to the backend `settings` table (see layoutStore.ts) -- so a
 * user's chosen width survives an app restart, same as the theme setting.
 *
 * This is intentionally a single resizable panel with a fixed sibling
 * (the rest of the row/column just grows via flexbox), not a full
 * multi-pane splitter grid -- that's all every required screen in the
 * spec actually needs (sidebar | main, main | assistant, explorer |
 * viewer, question | answer, etc.). Composing two `ResizablePanel`s
 * (or `ResizablePanel` + a plain flex-1 child) covers every listed case.
 */

export interface ResizablePanelProps {
  /** Stable identity for width persistence, e.g. "global.sidebar". */
  id: string;
  /** Width in px used the very first time this panel is ever rendered. */
  defaultWidth: number;
  minWidth?: number;
  maxWidth?: number;
  /**
   * Which edge the drag handle sits on. "end" means the handle is on the
   * trailing edge (panel is on the left, e.g. a left sidebar); "start"
   * means the handle is on the leading edge (panel is on the right, e.g.
   * a right-docked assistant panel).
   */
  handleSide?: "start" | "end";
  orientation?: "vertical" | "horizontal";
  className?: string;
  handleAriaLabel: string;
  children: ReactNode;
}

export function ResizablePanel({
  id,
  defaultWidth,
  minWidth = 200,
  maxWidth = 720,
  handleSide = "end",
  orientation = "vertical",
  className,
  handleAriaLabel,
  children,
}: ResizablePanelProps) {
  const storedWidth = useLayoutStore((s) => s.widths[id]);
  const setWidth = useLayoutStore((s) => s.setWidth);
  const resetWidth = useLayoutStore((s) => s.resetWidth);

  const width = useMemo(() => {
    const w = storedWidth ?? defaultWidth;
    return Math.min(maxWidth, Math.max(minWidth, w));
  }, [defaultWidth, maxWidth, minWidth, storedWidth]);

  const applyDelta = useCallback(
    (deltaPx: number) => {
      // A handle on the "start" edge grows the panel when dragged toward
      // the panel (i.e. leftward/upward shrinks a right-docked panel).
      const signed = handleSide === "start" ? -deltaPx : deltaPx;
      const next = Math.min(maxWidth, Math.max(minWidth, width + signed));
      setWidth(id, next);
    },
    [handleSide, id, maxWidth, minWidth, setWidth, width],
  );

  const handle = (
    <ResizeHandle
      orientation={orientation}
      onDrag={applyDelta}
      onKeyResize={applyDelta}
      onDoubleClick={() => resetWidth(id)}
      aria-label={handleAriaLabel}
      valueNow={width}
      valueMin={minWidth}
      valueMax={maxWidth}
    />
  );

  const style = orientation === "vertical" ? { width } : { height: width };

  return (
    <div
      className={cn("flex shrink-0", orientation === "vertical" ? "flex-row" : "flex-col", className)}
      style={orientation === "vertical" ? undefined : undefined}
    >
      {handleSide === "start" && handle}
      <div
        style={style}
        className={cn("min-w-0 shrink-0 overflow-hidden", orientation === "horizontal" && "min-h-0 w-full")}
      >
        {children}
      </div>
      {handleSide === "end" && handle}
    </div>
  );
}
