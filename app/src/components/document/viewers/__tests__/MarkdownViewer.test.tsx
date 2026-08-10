import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { extractHeadings, MarkdownViewer } from "@/components/document/viewers/MarkdownViewer";

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

describe("MarkdownViewer LaTeX rendering", () => {
  // Regression coverage: the document content renderer used to have no
  // math plugins at all (remark-math/rehype-katex were only wired into
  // the chat panel, not here), so inline/display math in a document's
  // own markdown rendered as literal `$...$` text instead of formatted
  // mathematics.
  it("renders inline math ($...$) as KaTeX output, not literal dollar text", () => {
    const { container } = render(
      <MarkdownViewer content="Einstein's equation: $E=mc^2$ is famous." zoom={1} searchQuery="" />,
    );
    expect(container.querySelector(".katex")).not.toBeNull();
    expect(container.textContent).not.toContain("$E=mc^2$");
  });

  it("renders display math ($$...$$) as KaTeX output, not literal text", () => {
    const { container } = render(
      <MarkdownViewer
        content={"Area under the curve:\n\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\n\nDone."}
        zoom={1}
        searchQuery=""
      />,
    );
    expect(container.querySelector(".katex")).not.toBeNull();
    expect(container.textContent).not.toContain("$$");
    expect(container.querySelector("annotation")?.textContent).toContain("\\int_0^1");
  });
});
