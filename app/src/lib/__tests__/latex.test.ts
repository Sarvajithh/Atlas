import { describe, expect, it } from "vitest";

import { normalizeLatexDelimiters } from "@/lib/latex";

describe("normalizeLatexDelimiters", () => {
  // Regression coverage for a real production bug: cloud/frontier-trained
  // models frequently use the `\( \)`/`\[ \]` convention regardless of
  // being instructed to use `$`/`$$`, and remark-math only understands
  // the latter -- so this math rendered as literal text even though the
  // KaTeX pipeline itself worked correctly.
  it("converts \\( ... \\) inline math to $...$", () => {
    expect(normalizeLatexDelimiters("The value \\( \\gamma \\) is the discount factor.")).toBe(
      "The value $\\gamma$ is the discount factor.",
    );
  });

  it("converts \\[ ... \\] display math to $$...$$", () => {
    expect(normalizeLatexDelimiters("\\[ v(s) = \\max_a R(s,a) \\]")).toBe("$$v(s) = \\max_a R(s,a)$$");
  });

  it("converts multiple occurrences in the same text", () => {
    const input = "Given \\( x \\) and \\( y \\), we have \\[ x + y = z \\].";
    const output = normalizeLatexDelimiters(input);
    expect(output).toContain("$x$");
    expect(output).toContain("$y$");
    expect(output).toContain("$$x + y = z$$");
  });

  it("leaves text with no backslash delimiters untouched", () => {
    const input = "Plain text with $already$ correct math and no other delimiters.";
    expect(normalizeLatexDelimiters(input)).toBe(input);
  });

  it("leaves an unmatched/unbalanced \\( untouched rather than guessing", () => {
    const input = "This has an unmatched \\( delimiter with no closing pair.";
    expect(normalizeLatexDelimiters(input)).toBe(input);
  });

  it("handles a multi-line display block", () => {
    const input = "\\[\nv(s) = \\max_a R(s,a)\n\\]";
    expect(normalizeLatexDelimiters(input)).toBe("$$v(s) = \\max_a R(s,a)$$");
  });
});
