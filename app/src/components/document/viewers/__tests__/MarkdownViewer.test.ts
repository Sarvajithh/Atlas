import { describe, expect, it } from "vitest";

import { extractHeadings } from "@/components/document/viewers/MarkdownViewer";

describe("extractHeadings", () => {
  it("extracts headings with correct depth", () => {
    const markdown = "# Title\n\nSome text.\n\n## Section One\n\n### Subsection\n\n## Section Two";
    const headings = extractHeadings(markdown);
    expect(headings.map((h) => [h.depth, h.text])).toEqual([
      [1, "Title"],
      [2, "Section One"],
      [3, "Subsection"],
      [2, "Section Two"],
    ]);
  });

  it("slugifies heading text into stable ids", () => {
    const headings = extractHeadings("# Hello World!");
    expect(headings[0].id).toBe("hello-world");
  });

  it("disambiguates duplicate heading text with a numeric suffix", () => {
    const headings = extractHeadings("# Intro\n\n# Intro");
    expect(headings[0].id).toBe("intro");
    expect(headings[1].id).toBe("intro-1");
  });

  it("returns an empty list for markdown with no headings", () => {
    expect(extractHeadings("Just a paragraph, no headings here.")).toEqual([]);
  });

  it("ignores '#' that isn't a heading (no following space)", () => {
    expect(extractHeadings("#nothashheading")).toEqual([]);
  });
});
