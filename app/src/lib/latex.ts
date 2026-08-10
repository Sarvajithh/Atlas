/**
 * Normalizes LaTeX math delimiters to the `$...$` / `$$...$$` convention
 * that `remark-math` (and therefore `rehype-katex`) actually understands.
 *
 * Real production bug this closes: prompting the model to use `$`/`$$`
 * delimiters (see `atlas-models::prompt_builder::MATH_FORMATTING_INSTRUCTION`)
 * is not sufficient on its own. Many models -- especially cloud-hosted
 * ones trained heavily on ChatGPT-style output -- have the `\( ... \)`
 * (inline) / `\[ ... \]` (display) convention baked in strongly enough to
 * use it regardless of instructions. `remark-math` does not recognize
 * that convention at all, so math wrapped in `\(`/`\[` rendered as
 * literal text even though the KaTeX pipeline itself was working
 * correctly -- indistinguishable from "LaTeX is broken" from the user's
 * side. Rather than relying entirely on prompting (which is inherently
 * best-effort against a model's own habits), this rewrites both
 * conventions into the one `remark-math` supports, at render time, so
 * either convention works regardless of which model produced the text.
 *
 * Deliberately conservative: only rewrites delimiter pairs that are
 * actually balanced and non-empty, and leaves everything else (including
 * any already-correct `$`/`$$` math, and any literal, unrelated
 * backslash-parenthesis text) untouched.
 */
export function normalizeLatexDelimiters(text: string): string {
  if (!text.includes("\\(") && !text.includes("\\[")) {
    return text;
  }

  let result = text;
  // Display math first (`\[ ... \]` -> `$$...$$`), since it's less
  // ambiguous and doing it first avoids the inline pass matching across
  // a display block's own brackets.
  result = result.replace(/\\\[([\s\S]*?)\\\]/g, (_match, inner: string) => {
    const trimmed = inner.trim();
    return trimmed ? `$$${trimmed}$$` : _match;
  });
  // Inline math (`\( ... \)` -> `$...$`).
  result = result.replace(/\\\(([\s\S]*?)\\\)/g, (_match, inner: string) => {
    const trimmed = inner.trim();
    return trimmed ? `$${trimmed}$` : _match;
  });

  return result;
}
