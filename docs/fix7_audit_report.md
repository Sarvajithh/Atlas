# Fix 7 — Audit for further silent integration gaps

Traced end-to-end (real inputs/real object graphs, not just each subsystem's own
unit-test fixtures), the same way the OCR/PDF defect (Fixes 2–3) was traced. No
code was changed as part of this pass — findings only, each ranked into the
existing P0–P3 scheme.

---

## Finding 1 (P0): the user's actual question/instruction is dropped from
## every prompt that goes through the Retriever pipeline

**Status:** Confirmed by direct code trace (not reproduced against a live
Ollama model in this environment — no model server available here — but the
prompt string handed to the Engine can be inspected without one, and it
settles the question).

**Evidence:**
- `atlas-models/src/prompt_builder.rs`, `PromptBuilder::build(&self, context:
  AssembledContext) -> ResolvedPrompt`. Its only input is the retrieved
  `context.hits`; the signature has no `query`/instruction parameter at all.
  The method body does exactly one thing: number and concatenate
  `context.hits[i].text_content` as `"[n] {text}"`, joined by blank lines.
- `atlas-models/src/scheduler.rs`, `ModelScheduler::execute`: for any pipeline
  containing `EngineRole::Retriever` (i.e. `Intent::FactualLookup`,
  `Intent::Tutoring`, `Intent::Quiz`, `Intent::Research` — everything except
  `Intent::Planning`), the flow is: retrieve hits -> `context_builder.assemble`
  -> `prompt_builder.build(context)` -> `engines.run_role(terminal_role,
  resolved)`. The `query: &str` parameter (the user's actual question, or —
  for Quiz/Flashcards — the constructed instruction string built in
  `AppFacade::quiz`/`flashcards`, e.g. "Generate 5 quiz questions (with
  answers) about: gradient descent. Base every question strictly on the
  provided context.") is used **only** as the search string passed to
  `retriever.retrieve(workspace_id, query, retrieval_limit)`. It is never
  folded into the prompt content itself.
- `atlas-core/src/facade.rs`, `AppFacade::chat_stream` (the streaming path used
  by the live Assistant Panel for ordinary tutoring conversations): identical
  shape -- `let resolved = self.prompt_builder.build(context); (resolved.content,
  images, citations)`. `message` (literally what the user typed) is discarded
  the same way. `AppFacade::search` (the `rag.*` IPC path) has the same gap.
- Net effect: for every Retriever-backed pipeline, the model receives a prompt
  that looks like:
  ```
  [1] <chunk text>

  [2] <chunk text>
  ```
  with no task description, no question, and no instruction of any kind. A
  model given only that has no way to know it's supposed to answer a
  question, generate a quiz, or produce flashcards -- it just sees an
  unlabeled numbered list of text fragments.
- Only `Intent::Planning` (Revision Planner) escapes this, because its
  pipeline (`[EngineRole::Memory, EngineRole::Planner]`) has no `Retriever`
  step, so `scheduler.execute`'s other branch uses
  `ResolvedPrompt::text(query)` directly -- the instruction *is* included
  there. This is why tracing Quiz/Flashcards/Revision Planner specifically
  surfaced the bug: Quiz and Flashcards both route through Retriever
  (broken), Revision Planner doesn't (fine).

**Why unit tests didn't catch it:** `PromptBuilder`'s own tests
(`build_numbers_each_chunk_as_an_inline_citation_marker` etc.) only assert the
`[n]` numbering format of context chunks -- they never construct a scenario
with a real instruction and assert it appears in the output, because the
function never received one to begin with. `ModelScheduler`'s tests use a
`StubEngine` that just records `prompt.content` and returns a canned string;
nothing in those tests asserts the original `query` text appears anywhere in
what the stub received. Both test suites pass today, and both would keep
passing even after a correct fix, because neither actually checks for the
presence of the instruction -- exactly the "isolated tests pass, real path
silently degrades" pattern this audit exists to find.

**Impact:** every Quiz Generator call, every Flashcard Generator call, and
every ordinary tutoring/Q&A chat turn that has any retrieved context at all
(the overwhelming majority of real usage) sends the model a prompt with no
task framing. A capable instruction-tuned model may sometimes produce
something plausible-looking from bare context alone (e.g. summarizing it),
which would make this easy to miss in casual manual testing -- but it is not
answering the user's actual question, generating the requested number of quiz
questions, or following "base every question strictly on the provided
context," because it never sees any of that text.

**Repro:** call `PromptBuilder::build` (or run `AppFacade::quiz`/`chat_stream`
against any workspace with indexed content) and inspect `ResolvedPrompt.content`
-- it contains only numbered context chunks, never the `query`/instruction
string, regardless of what was asked.

**Suggested scope for a real fix** (not implemented here, per this task's
scope): thread `query`/the instruction string into `PromptBuilder::build` (or
a template it's given) and place it in the prompt -- clearly separated from
the numbered context (e.g. instruction first, then "Context:" followed by the
numbered chunks) -- for every pipeline branch that currently drops it
(`scheduler.rs`'s Retriever branch, `facade.rs::chat_stream`'s Retriever
branch, and `facade.rs::search`). This is a wider-reaching fix than Fixes 2-3
required, since `PromptBuilder::build`'s signature itself needs to change and
every call site updated -- flagged for deliberate scoping, not attempted
inline here per this fix's own instructions.

---

## Finding 2 (P1): the Folder Watcher's debounce window silently collapses to
## the ~50ms poll interval shortly after a watcher starts, defeating real
## debouncing

**Status:** Confirmed by direct code trace; not reproduced against a live
timed filesystem burst in this environment (would need a longer-running,
timing-sensitive integration test than was practical to add for a
report-only pass), but the bug is structural, not a timing coincidence, so
the trace is conclusive on its own.

**Evidence:**
- `atlas-watcher/src/watcher.rs`, inside `FolderWatcher::watch`'s `notify`
  callback closure:
  ```rust
  let observed_at_ms = Instant::now().elapsed().as_millis() as u64;
  ```
  `Instant::now()` constructs a brand-new `Instant` right there, and
  `.elapsed()` is called on it immediately -- this measures the time between
  those two lines executing, not time since the watcher started. In practice
  this is consistently ~0.
- Meanwhile, in the debounce thread also spawned by `watch`:
  ```rust
  let start = Instant::now();
  let now_ms = || start.elapsed().as_millis() as u64;
  ...
  for (path, kind) in debouncer.drain_ready(now_ms()) { ... }
  ```
  `start` is fixed once, when the thread begins, and `now_ms()` grows for the
  entire lifetime of the watcher.
- `Debouncer::drain_ready(now_ms)` (`atlas-watcher/src/debounce.rs`) treats a
  pending change as ready once `now_ms.saturating_sub(last_seen) >=
  self.window_ms`, where `last_seen` is exactly the `observed_at_ms` computed
  above (approx 0).
- Consequence: once the watcher has been running for longer than
  `debounce_window_ms` (default 500ms -- so after the watcher's first
  half-second of uptime), every newly observed change is already "ready" the
  very next time the debounce thread polls it (the poll loop runs every
  50ms), because `now_ms() - 0` is already far past the window. The
  configured debounce window is only actually honored during the watcher's
  first `window_ms` of uptime; after that it silently degrades to roughly the
  50ms poll interval.
- This does not break coalescing of near-simultaneous events for the same
  path within one 50ms poll tick (`Debouncer::observe` still correctly
  collapses multiple raw events into one pending entry via the `HashMap`
  `insert`), but it defeats the actual purpose of a 500ms debounce window for
  bursts spread across multiple poll ticks -- e.g. an editor doing several
  saves a few hundred milliseconds apart, or a sync client rewriting a file in
  stages, will each trigger their own separate enqueued indexing job instead
  of being coalesced into one, once the watcher has been running a while.

**Why unit tests didn't catch it:** `debounce.rs`'s own tests
(`single_created_event_debounces_to_added_after_window` etc.) construct
`RawChange { observed_at_ms: <hand-picked value>, .. }` directly and drive
`now_ms` with hand-picked values from the same convention -- they never
involve the real `Instant`-based clock computation in `watcher.rs` at all, so
this bug lives entirely in the seam between the two files, which no existing
test crosses. `watcher.rs`'s own real-filesystem test
(`watch_then_creating_a_file_eventually_publishes_and_enqueues`) only asserts
that some event is eventually published within a several-second poll window --
it doesn't create a burst of changes over time and assert they coalesce into
one job, so a debounce window that's silently much shorter than configured
still passes it.

**Impact:** the Folder Watcher -> indexing job-queue hand-off itself works (a
real file create/modify/delete does reliably produce a real enqueued job --
confirmed by re-running the existing real-filesystem test successfully, and
`publish_and_enqueue`'s logic is straightforward and correct). The gap is
specifically that debouncing of rapid successive changes silently stops
working as configured shortly after each watcher starts, which could mean
more indexing churn (redundant jobs, wasted parse/chunk/embed work, partially
absorbed by `content_hash`-driven skips for a genuinely-unchanged re-save --
see `pipeline.rs`'s `reindexing_unchanged_file_is_skipped`) than the 500ms
window was intended to prevent, rather than a correctness failure of indexing
itself.

**Suggested scope for a real fix:** compute `observed_at_ms` from the same
`start: Instant` the debounce thread already owns (e.g. share it into the
`notify` callback closure via the existing `move` closure, the same way
`events`/`jobs`/`debounce_window_ms` are already moved in), rather than a
throwaway `Instant::now().elapsed()`.

---

## Finding 3 (P0): the DOCX parser cannot read real Word-produced files --
## confirmed by direct repro, not just suspicion

**Status:** Confirmed by direct repro against a synthetically-built but
realistically-encoded `.docx` file (Python's `zipfile` module with
`ZIP_DEFLATED`, matching how Word/LibreOffice/python-docx actually write
`word/document.xml` by default).

**Evidence:**
- `atlas-indexer/src/parser.rs`, `pub mod docx`, `find_stored_document_xml`:
  the module's own doc comment already discloses the limitation honestly --
  "This only succeeds when that entry's compression method is 0 (stored);
  most other cases fall back gracefully" -- but real Word/LibreOffice/
  python-docx output almost never uses STORED (uncompressed) entries; they
  DEFLATE-compress `word/document.xml` by default, the same underlying issue
  Fix 2 diagnosed and fixed for PDF content streams.
- **Repro performed**: built two otherwise-identical minimal `.docx`-shaped
  ZIP files, one with `ZIP_STORED` (uncompressed -- what this parser can
  read) and one with `ZIP_DEFLATED` (compressed -- what real Word actually
  produces), both containing `word/document.xml` with the text "Hello real
  Word text". Ran `atlas_indexer::parser::docx::parse_docx_bytes` against
  both:
  ```
  real_word_style.docx (DEFLATE): 1 block(s), first block type/text = Image / ""
  stored_style.docx    (STORED):  1 block(s), first block type/text = Paragraph / "Hello real Word text"
  ```
  The DEFLATE-compressed file -- the realistic case -- degrades to a single
  empty `Image` block, losing all text content, exactly mirroring the PDF bug
  Fix 2 addressed. The STORED file (atypical for a real Word export) is read
  correctly.
- This is not hypothetical: `find_stored_document_xml` literally checks
  `compression == 0` and returns `None` otherwise, with the caller
  (`parse_docx_bytes`) falling back to the same "single empty Image block,
  never silently dropped, but also never actually read" degradation Fix 2 had
  to fix for PDFs.

**Why unit tests didn't catch it:** every existing `docx::tests` fixture
(`extract_paragraphs_reads_runs_within_a_paragraph`,
`multiple_paragraphs_are_split`,
`unreadable_container_falls_back_to_a_single_image_block_not_an_error`) either
calls `extract_paragraphs` directly on a raw XML string (bypassing ZIP/
compression entirely) or constructs a deliberately-broken container to test
the fallback path -- none of them build a realistically-compressed ZIP and
assert text comes back, so the gap between "well-formed XML -> correct
paragraphs" (tested) and "real compressed .docx file -> correct paragraphs"
(never tested, and broken) was invisible to the existing suite.

**Impact:** essentially all real-world `.docx` uploads (Word, Google Docs
exported as `.docx`, LibreOffice, python-docx-generated files) will silently
extract zero text and get flagged for OCR instead -- which won't help, since
OCR is for scanned raster images and a DOCX has no embedded page image to
rasterize/extract (`DocxParser` doesn't implement `extract_ocr_image` at all,
so `pipeline.rs`'s fallback would read the raw `.docx` ZIP bytes as "the
image," which no OCR engine can decode any better than the equivalent PDF
case Fix 3 fixed). Post-Fix-5, this at least now surfaces as `ParsedEmpty` in
the UI instead of silently looking identical to a successful index -- but the
underlying extraction gap itself remains unfixed.

**Suggested scope for a real fix:** the same shape as Fix 2 -- replace the
hand-rolled ZIP/XML scanner with a real ZIP reader (`atlas-indexer` already
gained `flate2` as a dependency for Fix 2's PDF FlateDecode support, exactly
the codec needed here too; a minimal ZIP central-directory walk plus `flate2`
inflate would cover the common case without a full third-party ZIP crate,
though the `zip` crate is also a reasonable, well-maintained pure-Rust option
per the same "prefer a vetted library over further hand-rolling" reasoning
Fix 2 used for PDFs).

---

## Summary

| # | Area | Status | Priority |
|---|------|--------|----------|
| 1 | User's question/instruction dropped from every Retriever-backed prompt (chat, Quiz, Flashcards, Research, factual lookup) | Confirmed by code trace | **P0** |
| 2 | Folder Watcher debounce window silently collapses to ~poll-interval after startup | Confirmed by code trace | P1 |
| 3 | DOCX parser cannot read real (DEFLATE-compressed) Word output | Confirmed by direct repro | **P0** |

Finding 1 is, in practical terms, likely the most severe of the three -- it
affects the core tutoring/chat experience itself, not just a specific
document-format or watcher edge case, and unlike Findings 2-3 it has no
graceful degradation path (the model isn't told what to do at all, rather
than just missing some content it needed). None of these three were fixed as
part of this pass, per this task's explicit scope for Fix 7 -- they're
reported here for deliberate scoping and prioritization, the same way Fixes
1-6 originally were.
