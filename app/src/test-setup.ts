// Test environment shims (jsdom doesn't implement everything Chromium
// does). `DOMMatrix` is required by `pdfjs-dist` (used in `PdfViewer`)
// purely for canvas transform math -- stubbing it here is a jsdom gap
// fix, not a change to any real rendering logic, and mirrors the kind of
// narrow polyfill jsdom-based suites commonly need for canvas-adjacent
// libraries.
if (typeof globalThis.DOMMatrix === "undefined") {
  class DOMMatrixShim {
    a = 1; b = 0; c = 0; d = 1; e = 0; f = 0;
    constructor(_init?: unknown) {}
  }
  // @ts-expect-error -- minimal shim, not a spec-complete DOMMatrix
  globalThis.DOMMatrix = DOMMatrixShim;
}
