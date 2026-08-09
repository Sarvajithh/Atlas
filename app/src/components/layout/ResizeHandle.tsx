import { useCallback, useRef } from "react";

import { cn } from "@/state/utils";

/**
 * Global Resizable Panel System -- draggable divider.
 *
 * Used internally by `ResizablePanel`, but exported standalone in case a
 * consumer needs a divider between two panels it already manages itself
 * (e.g. two panels that both grow, with no single "resizable" side).
 *
 * Handles: mouse drag, touch drag, keyboard (arrow keys), and
 * double-click-to-reset. Purely a controlled input -- it reports deltas
 * and events, it does not own width state itself (that lives in
 * `ResizablePanel` / `layoutStore`).
 */

export interface ResizeHandleProps {
  /** Which edge of the panel this handle sits on / which axis it resizes. */
  orientation?: "vertical" | "horizontal";
  /** Called continuously while dragging with the raw pointer delta in px. */
  onDrag: (deltaPx: number) => void;
  /** Called once when a drag gesture finishes (mouseup/touchend). */
  onDragEnd?: () => void;
  /** Called on double-click, e.g. to reset to the default width. */
  onDoubleClick?: () => void;
  /** Called on ArrowLeft/Right (vertical) or ArrowUp/Down (horizontal). */
  onKeyResize?: (deltaPx: number) => void;
  "aria-label": string;
  /** Current size, exposed to assistive tech via aria-valuenow. */
  valueNow?: number;
  valueMin?: number;
  valueMax?: number;
  className?: string;
}

const KEYBOARD_STEP_PX = 16;

export function ResizeHandle({
  orientation = "vertical",
  onDrag,
  onDragEnd,
  onDoubleClick,
  onKeyResize,
  valueNow,
  valueMin,
  valueMax,
  className,
  ...aria
}: ResizeHandleProps) {
  const lastPos = useRef<number>(0);
  const dragging = useRef(false);

  const posFromEvent = useCallback(
    (e: MouseEvent | TouchEvent) => {
      const point = "touches" in e ? e.touches[0] ?? e.changedTouches[0] : e;
      return orientation === "vertical" ? point.clientX : point.clientY;
    },
    [orientation],
  );

  const handleMove = useCallback(
    (e: MouseEvent | TouchEvent) => {
      if (!dragging.current) return;
      const pos = posFromEvent(e);
      const delta = pos - lastPos.current;
      lastPos.current = pos;
      if (delta !== 0) onDrag(delta);
    },
    [onDrag, posFromEvent],
  );

  const stopDragging = useCallback(() => {
    if (!dragging.current) return;
    dragging.current = false;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    window.removeEventListener("mousemove", handleMove);
    window.removeEventListener("mouseup", stopDragging);
    window.removeEventListener("touchmove", handleMove);
    window.removeEventListener("touchend", stopDragging);
    onDragEnd?.();
  }, [handleMove, onDragEnd]);

  const startDragging = useCallback(
    (e: React.MouseEvent | React.TouchEvent) => {
      dragging.current = true;
      lastPos.current = posFromEvent(e.nativeEvent as MouseEvent | TouchEvent);
      document.body.style.cursor = orientation === "vertical" ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", handleMove);
      window.addEventListener("mouseup", stopDragging);
      window.addEventListener("touchmove", handleMove, { passive: false });
      window.addEventListener("touchend", stopDragging);
    },
    [handleMove, orientation, posFromEvent, stopDragging],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!onKeyResize) return;
      const isVertical = orientation === "vertical";
      if ((isVertical && e.key === "ArrowLeft") || (!isVertical && e.key === "ArrowUp")) {
        e.preventDefault();
        onKeyResize(-KEYBOARD_STEP_PX);
      } else if ((isVertical && e.key === "ArrowRight") || (!isVertical && e.key === "ArrowDown")) {
        e.preventDefault();
        onKeyResize(KEYBOARD_STEP_PX);
      } else if (e.key === "Enter" && onDoubleClick) {
        e.preventDefault();
        onDoubleClick();
      }
    },
    [onDoubleClick, onKeyResize, orientation],
  );

  return (
    <div
      role="separator"
      aria-orientation={orientation === "vertical" ? "vertical" : "horizontal"}
      aria-valuenow={valueNow}
      aria-valuemin={valueMin}
      aria-valuemax={valueMax}
      tabIndex={0}
      onMouseDown={startDragging}
      onTouchStart={startDragging}
      onDoubleClick={onDoubleClick}
      onKeyDown={onKeyDown}
      className={cn(
        "group relative shrink-0 touch-none select-none bg-transparent",
        orientation === "vertical" ? "w-1.5 cursor-col-resize" : "h-1.5 cursor-row-resize",
        "focus-visible:outline-none",
        className,
      )}
      {...aria}
    >
      <div
        className={cn(
          "absolute rounded-full bg-border transition-colors group-hover:bg-primary/50 group-focus-visible:bg-primary",
          orientation === "vertical"
            ? "inset-y-0 left-1/2 w-px -translate-x-1/2"
            : "inset-x-0 top-1/2 h-px -translate-y-1/2",
        )}
      />
    </div>
  );
}
