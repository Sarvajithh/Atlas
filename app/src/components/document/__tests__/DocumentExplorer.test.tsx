import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { DocumentExplorer } from "@/components/document/DocumentExplorer";
import type { DocumentRecord } from "@/ipc/types";

function makeDoc(overrides: Partial<DocumentRecord> = {}): DocumentRecord {
  return {
    id: 1,
    workspace_id: 1,
    relative_path: "notes/chapter1.md",
    content_hash: "hash",
    file_type: "md",
    size: 100,
    mtime: "2026-01-01T00:00:00Z",
    parse_status: "Parsed",
    last_indexed_hash: null,
    ...overrides,
  };
}

describe("DocumentExplorer", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("shows an empty state when there are no indexed documents", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    render(<DocumentExplorer workspaceId={1} onOpenDocument={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("No documents indexed yet")).toBeTruthy());
  });

  it("groups documents into folders derived from relative_path", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      makeDoc({ id: 1, relative_path: "notes/chapter1.md" }),
      makeDoc({ id: 2, relative_path: "syllabus.pdf", file_type: "pdf" }),
    ]);
    render(<DocumentExplorer workspaceId={1} onOpenDocument={vi.fn()} />);
    const notesFolder = await screen.findByText("notes");
    expect(screen.getByText("syllabus.pdf")).toBeTruthy();
    // Nested folders collapse by default; expand to reveal chapter1.md.
    await userEvent.click(notesFolder);
    expect(screen.getByText("chapter1.md")).toBeTruthy();
  });

  it("calls onOpenDocument with the real DocumentRecord when a file is clicked", async () => {
    const doc = makeDoc({ id: 5, relative_path: "syllabus.pdf", file_type: "pdf" });
    vi.mocked(invoke).mockResolvedValueOnce([doc]);
    const onOpen = vi.fn();
    render(<DocumentExplorer workspaceId={1} onOpenDocument={onOpen} />);
    const fileButton = await screen.findByText("syllabus.pdf");
    await userEvent.click(fileButton);
    expect(onOpen).toHaveBeenCalledWith(doc);
  });

  it("filters documents by the filter input", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      makeDoc({ id: 1, relative_path: "chapter1.md" }),
      makeDoc({ id: 2, relative_path: "syllabus.pdf", file_type: "pdf" }),
    ]);
    render(<DocumentExplorer workspaceId={1} onOpenDocument={vi.fn()} />);
    await screen.findByText("chapter1.md");
    await userEvent.type(screen.getByLabelText("Filter files"), "syllabus");
    expect(screen.queryByText("chapter1.md")).toBeNull();
    expect(screen.getByText("syllabus.pdf")).toBeTruthy();
  });

  // Fix 5 (P1 audit): a document that finished indexing without error but
  // extracted zero chunks must be visibly distinguishable in the tree from
  // both a normal successfully-parsed file and a hard `Failed` one.
  it("shows a distinct indicator and tooltip for a document with no text extracted", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      makeDoc({ id: 3, relative_path: "blank-scan.pdf", file_type: "pdf", parse_status: "ParsedEmpty" }),
    ]);
    render(<DocumentExplorer workspaceId={1} onOpenDocument={vi.fn()} />);
    const fileButton = await screen.findByText("blank-scan.pdf");
    const button = fileButton.closest("button");
    expect(button?.getAttribute("title")).toBe("No text extracted from this file");
    expect(button?.textContent).toContain("∅");
  });
});
