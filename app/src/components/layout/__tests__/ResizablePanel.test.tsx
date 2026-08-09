import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

vi.mock("@/ipc/settings", () => ({
  settingsGet: vi.fn().mockResolvedValue(null),
  settingsSet: vi.fn().mockResolvedValue(undefined),
}));

import { settingsSet } from "@/ipc/settings";
import { useLayoutStore } from "@/components/layout/layoutStore";
import { ResizablePanel } from "@/components/layout/ResizablePanel";

/**
 * Global Resizable Panel System tests (V1.0 Part 1). Exercises real drag
 * math and real persistence calls into the mocked `settings.*` IPC layer
 * (matching the pattern used by `state/theme.ts`'s own tests), not a
 * fabricated stand-in for the resize behavior.
 */
describe("ResizablePanel", () => {
  beforeEach(() => {
    useLayoutStore.setState({ widths: {}, isLoaded: false });
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  it("renders children at the default width when nothing is persisted", () => {
    render(
      <ResizablePanel id="test.panel" defaultWidth={300} handleAriaLabel="Resize test panel">
        <div data-testid="content">content</div>
      </ResizablePanel>,
    );
    expect(screen.getByTestId("content")).toBeTruthy();
    const container = screen.getByTestId("content").parentElement as HTMLElement;
    expect(container.style.width).toBe("300px");
  });

  it("grows the panel when the handle is dragged toward handleSide=end", () => {
    render(
      <ResizablePanel
        id="test.panel"
        defaultWidth={300}
        minWidth={100}
        maxWidth={600}
        handleSide="end"
        handleAriaLabel="Resize test panel"
      >
        <div data-testid="content">content</div>
      </ResizablePanel>,
    );

    const handle = screen.getByRole("separator", { name: "Resize test panel" });
    fireEvent.mouseDown(handle, { clientX: 300 });
    fireEvent.mouseMove(window, { clientX: 350 });
    fireEvent.mouseUp(window);

    expect(useLayoutStore.getState().widths["test.panel"]).toBe(350);
  });

  it("clamps to minWidth/maxWidth", () => {
    render(
      <ResizablePanel
        id="test.panel"
        defaultWidth={300}
        minWidth={280}
        maxWidth={320}
        handleSide="end"
        handleAriaLabel="Resize test panel"
      >
        <div data-testid="content">content</div>
      </ResizablePanel>,
    );
    const handle = screen.getByRole("separator", { name: "Resize test panel" });
    fireEvent.mouseDown(handle, { clientX: 300 });
    fireEvent.mouseMove(window, { clientX: 1000 });
    fireEvent.mouseUp(window);
    expect(useLayoutStore.getState().widths["test.panel"]).toBe(320);
  });

  it("double-click resets to default width and persists the reset", () => {
    useLayoutStore.setState({ widths: { "test.panel": 500 }, isLoaded: true });
    render(
      <ResizablePanel id="test.panel" defaultWidth={300} handleAriaLabel="Resize test panel">
        <div data-testid="content">content</div>
      </ResizablePanel>,
    );
    const handle = screen.getByRole("separator", { name: "Resize test panel" });
    fireEvent.doubleClick(handle);
    expect(useLayoutStore.getState().widths["test.panel"]).toBeUndefined();
    expect(settingsSet).toHaveBeenCalledWith(
      expect.objectContaining({ key: "ui.layout.panelWidths" }),
    );
  });

  it("debounces persistence of a dragged width to the real settings IPC", () => {
    render(
      <ResizablePanel id="test.panel" defaultWidth={300} handleAriaLabel="Resize test panel">
        <div data-testid="content">content</div>
      </ResizablePanel>,
    );
    const handle = screen.getByRole("separator", { name: "Resize test panel" });
    fireEvent.mouseDown(handle, { clientX: 300 });
    fireEvent.mouseMove(window, { clientX: 320 });
    fireEvent.mouseUp(window);

    expect(settingsSet).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(settingsSet).toHaveBeenCalledWith(
      expect.objectContaining({ key: "ui.layout.panelWidths", value: JSON.stringify({ "test.panel": 320 }) }),
    );
  });
});
