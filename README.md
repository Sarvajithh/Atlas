# Atlas

Local Learning OS — a local-first, offline, single-user AI Learning
Operating System for desktop.

**The architecture contract lives at [`app/docs/README.md`](app/docs/README.md).**
It is the single source of truth for the project's *design* (frozen, per its
own §0). This file is the single source of truth for the project's
*implementation status* — it is kept current after every implementation
phase and should never be allowed to drift behind the code.

## Current milestone

This repository is **past the skeleton stage**. The Cargo workspace, crate
boundaries, IPC command shapes, frontend scaffold, and tooling config exist
*and* a substantial amount of real business logic has been implemented on
top of them: document parsing, OCR (including vision-model OCR for
handwriting), chunking, embeddings, hybrid retrieval, reranking, RAG prompt
construction, Ollama-backed chat/tutoring with streaming, a folder watcher
with incremental indexing, and a working React/Tauri frontend shell with
IPC wiring for workspaces, documents, and the assistant.

**Estimated overall completion toward the Atlas v1.0 vision: ~48%.**
See "Completed Features," "Remaining Atlas v1.0 Work," and "Known
Limitations & Technical Debt" below for the itemized breakdown this
estimate is based on.

This status reflects a full source-level engineering audit (see
`docs/atlas_eda_audit.md` if present, or the audit delivered alongside this
README) — every claim below was verified against actual source files, not
inferred from comments or prior documentation.

---

## Completed Features

### Workspaces
- Workspace lifecycle (Unlinked → Linking → Indexing → Active → Archived),
  backed by SQLite (`atlas-workspace`, `atlas-db::workspace_adapter`)
- Multi-workspace support (backend + `WorkspaceHome` list view)
- Folder linking (`WorkspaceCreationWizard` + IPC)
- Folder watching with incremental indexing (`atlas-watcher`,
  `atlas-core::worker`) — functional; the debounce window now correctly
  coalesces bursts across the watcher's whole uptime (see Changelog), fixing
  a prior known bug
- Workspace explorer / file tree with indexing-status badges (`Sidebar`,
  `DocumentExplorer`)
- Workspace dashboard (`WorkspaceHome`, `WorkspaceDetail`)

### Document System
- PDF parsing (real content-stream/FlateDecode extraction, the largest
  single module in the codebase)
- Vision-model OCR for handwritten pages, with Tesseract fallback
  (`atlas-models::vision_ocr`)
- Image OCR (Tesseract CLI, `atlas-indexer::ocr`)
- Markdown viewing (tested)
- Document metadata (`atlas-types::document`, `document_adapter`)
- DOCX parsing (DEFLATE-compressed, real Word output) — reads both STORED
  and DEFLATE-compressed `word/document.xml` entries, so real
  Word/LibreOffice/python-docx exports (not just uncompressed ZIPs) now
  extract text correctly; see Changelog

### Knowledge Ingestion & RAG
- Chunking (`atlas-indexer::chunker`)
- Embeddings (`atlas-models::embedding`, `atlas-indexer::embedding`)
- Hybrid retrieval with a wide candidate pool before reranking
  (`atlas-models::retriever`)
- Lightweight term-overlap + phrase-bonus reranker (`atlas-models::reranker`
  — not a cross-encoder; documented as an intentional lighter-weight
  alternative)
- Structured RAG prompt construction: SYSTEM / WORKSPACE CONTEXT / USER
  QUESTION / ANSWER, settings-overridable system prompt, no hardcoded
  prompt strings in engine code (`atlas-models::prompt_builder`) — the
  user's actual question is correctly included in every prompt (this was
  previously a P0 defect; confirmed fixed in current source)
- Context compression: near-duplicate chunk removal, adjacent-chunk merging
  (`atlas-models::context_builder`)
- Citation markers (`[1]`, `[2]`, ...) matching numbered context
  (`atlas-models::citation`)
- Background job queue + worker for indexing (`atlas-indexer::job_queue`,
  `atlas-core::worker`)

### AI System
- Model Registry with role-based discovery (`atlas-models::registry`,
  `discovery`) — no hardcoded model names found anywhere in engine code
- Ollama integration, streaming chat responses run off the main thread
  (`commands::assistant::assistant_ask_stream`)
- Engine role dispatch (`EnginePool`, `EngineRole`) for
  Tutor/Reasoning/Vision/Planner routing

### Student Memory
- Annotations, bookmarks, chat history, and progress/analytics repositories
  (`atlas-memory`), following the architecture contract's Student Memory
  non-destructive-deletion guarantee

### UI Shell
- Shell layout matching the architecture contract's wireframe: Activity
  Rail, Sidebar, Tabs, main document area, dockable Assistant Panel, Status
  Bar
- PDF viewer, Markdown viewer, document tabs, split view
  (Ctrl/Cmd+\\), assistant panel toggle (Ctrl/Cmd+B)
- Theme support, Settings view
- **Frontend navigation wiring** — `ConceptGraphView`, `ResearchMode`,
  `QuizExamMode`, `MemoryAnalyticsView`, and `DocumentView` are now routed
  and reachable from the running shell (`ActivityRail` for Concept Graph
  and Memory & Analytics; a workspace-scoped mode switcher in `TopNav` for
  Document View, Research Mode, and Quiz/Exam Mode) — see Changelog. This
  only wires navigation; it does not claim the underlying feature logic
  behind each view is complete. Most still render an honest empty/loading
  state pending their own backend surfaces (Concept Graph extraction,
  Research Mode's cross-document linking, Quiz/Flashcard generation depth,
  Memory & Analytics' aggregation queries) — each remains tracked below
  under its own item, not silently marked done here.

---

## Remaining Atlas v1.0 Work

*(Copied forward from the full engineering audit; nothing removed except
items independently re-verified as already done above.)*

### High priority
- **Global Search** — required by the architecture contract's navigation
  flow (§9); no unified hybrid-search IPC command or frontend surface
  exists yet, despite the underlying retrieval/reranking machinery being
  reusable for it.
- **Concept Graph construction/extraction logic** — the crate currently
  contains only repository interfaces and injected-dependency scaffolding;
  its own code comment explicitly states extraction logic is "deferred to
  a future milestone." No nodes/edges are ever produced today.

### Medium priority
- **Quiz / Flashcard / Revision Planner generation depth** — currently
  one-line wrappers around a generic LLM call with no structured output
  schema, no persisted structured records, and no real weak-topic-detection
  computation behind them.
- **Research Mode features** — Literature Review, Paper Comparison,
  Citation Graph, Timeline, and general cross-document/cross-workspace
  linking are entirely absent from the codebase (zero occurrences of these
  terms anywhere in source).
- **Vector store vs. architecture contract** — the current implementation
  is a custom in-house `EmbeddedVectorStore`, not Qdrant or LanceDB as
  mandated by the frozen architecture contract §5. This needs either a
  formal contract amendment or a migration.

### Lower priority / feature completion
- Explicit HTML parsing (no dedicated module found)
- Table, figure, and formula *extraction* from documents (none found;
  citation *generation* in RAG answers exists, but document-level citation
  extraction does not)
- Mind Maps, Formula Sheets, Study Guides — entirely absent
- Formula rendering in the frontend — no math-rendering library found
- Explicit "rebuild index" action — only incremental, event-driven indexing
  exists
- Model Dashboard view — no standalone surface for reviewing Model
  Registry assignments per engine role
- Resizable panel support — not independently confirmed present
- Conversation memory threaded into multi-turn prompts — chat history is
  stored, but the prompt template does not appear to include prior-turn
  context
- Fine-grained RAG context scoping (current-page, selected-text) — only
  workspace-level scoping confirmed

---

## Known Limitations & Technical Debt

- **Reranker is a lightweight heuristic** (term overlap + phrase bonus),
  not a cross-encoder model — documented as an intentional scope choice in
  the source, not a hidden gap.
- **Test coverage gap pattern**: prior internal audits of this repository
  identified specific reasons existing unit tests didn't catch the DOCX and
  watcher bugs (fixtures that bypass the real compressed/ZIP path;
  timing-independent debounce tests). This phase closed both gaps for the
  DOCX and watcher fixes — a real DEFLATE-compressed ZIP fixture (built via
  an actual `flate2` encoder, not a raw XML string) and a watcher-level test
  that runs the debounce window to completion before firing a multi-tick
  burst — see Changelog. The same "isolated tests pass, real path silently
  degrades" pattern is still unaddressed for the previously-identified
  dropped-user-query-from-prompts issue (see Finding 1 in
  `docs/fix7_audit_report.md`), which remains out of this phase's scope.
- **Architecture contract deviation**: custom vector store instead of the
  mandated Qdrant/LanceDB (§5 of `app/docs/README.md`) — unresolved,
  unamended as of this writing.
- **Missing devDependency**: `tailwind.config.js` imports
  `@tailwindcss/typography`, but it was absent from `package.json`/
  `package-lock.json` (pre-existing gap, not introduced by any phase's
  changes) — this silently broke `npm test`/`npm run build` in any
  environment without it already present in `node_modules` some other way.
  Added as a devDependency in the frontend-navigation phase since it
  blocked verifying that phase's own acceptance criteria; flagged here as
  a deviation from that phase's stated file scope, since `package.json`
  wasn't on its allowed-to-change list.
- Two internal audit artifacts (`SUMMARY.md`, `docs/fix7_audit_report.md`,
  `CHANGES.diff`) exist at/near the repository root from prior fix passes;
  useful history, but should eventually be consolidated into a single
  changelog rather than left as standalone documents.

---

## Implementation Status by Subsystem

| Subsystem | Estimated completion |
|---|---|
| Workspaces | ~68% |
| Document System | ~55% |
| Knowledge Ingestion | ~60% |
| AI System (core chat) | ~65% |
| RAG | ~55% |
| Learning | ~20% |
| Research | ~10% |
| UI (shell + navigation) | ~50% |
| Performance | ~45% |
| **Overall** | **~48%** |

---

## Configuration

No new configuration surfaces beyond what already exists in
`atlas-config`/Settings (e.g. `assistant.system_prompt_template`) have been
added as part of this audit. See `app/docs/README.md` for the frozen
settings/configuration model.

## Benchmarks

No new benchmark numbers were produced by this audit (documentation-only
pass; no code was modified). See `SUMMARY.md` for the most recent
prompt-quality/latency benchmark predictions, which have not been
independently re-verified against a live Ollama instance in this
environment.

## Changelog

- **[Frontend navigation pass, this entry]** — Wired the 5 previously
  built-but-unreachable views into the shell (`ConceptGraphView`,
  `ResearchMode`, `QuizExamMode`, `MemoryAnalyticsView`, `DocumentView`),
  no new feature logic behind any of them:
  - `state/store.ts`'s `AppView` union gained `"concept-graph"`,
    `"research-mode"`, `"quiz-exam"`, `"memory-analytics"`, and
    `"document-view"`.
  - `App.tsx`'s `mainContent` conditional now renders each, following the
    exact same pattern already used for `"settings"`/`"workspace-detail"`.
  - `ActivityRail.tsx`: the previously-disabled Concept Graph and Memory &
    Analytics rail buttons are now enabled and route to their views. A
    Global Search rail entry was deliberately not added — no backend
    surface exists for it yet (see "Remaining Atlas v1.0 Work").
  - `TopNav.tsx`: added a workspace-scoped mode switcher (Explorer /
    Document View / Research Mode / Quiz-Exam) shown once a workspace is
    open, per §8.1's "Main Document Area ... shows the current document or
    the current mode" — this is the reachable entry point for the 3
    workspace-context views.
  - No router library was introduced; view switching stayed on the
    existing `currentView` state-based approach, which was structurally
    sufficient for this.
  - None of the 5 views' internal logic was touched — each still renders
    its pre-existing honest empty/stub state (no backing IPC calls exist
    yet for any of them), which is expected for this phase, not a defect.
  - Added `App.test.tsx` (navigation-level tests covering all 5 new routes
    plus a no-regression check on Settings/Dashboard) and confirmed no
    existing view regressed.

- **[Bugfix pass, prior entry]** — Fixed the two P0/P1 backend defects
  identified by the prior engineering audit:
  - **DOCX parser**: `find_document_xml` (formerly
    `find_stored_document_xml`) in `atlas-indexer::parser::docx` now
    decompresses `word/document.xml` ZIP entries using DEFLATE (compression
    method 8, via `flate2::read::DeflateDecoder`), in addition to the
    pre-existing STORED (method 0) fast path. Real Word/LibreOffice/
    python-docx output — which is DEFLATE-compressed — now extracts real
    paragraph text instead of degrading to an empty `Image` block. Added a
    unit test that builds a genuinely DEFLATE-compressed synthetic `.docx`

    fixture (via `flate2::write::DeflateEncoder`, not a raw XML string) and
    asserts real text comes back, plus a regression test confirming the
    STORED fast path still works.
  - **Folder watcher debounce**: `FolderWatcher::watch` in
    `atlas-watcher::watcher` previously computed `observed_at_ms` from a
    freshly-constructed `Instant::now().elapsed()` (always ≈0) in the
    `notify` callback, while the debounce thread's `now_ms()` measured
    elapsed time from its own `start: Instant` fixed at thread spawn — two
    different clocks being compared, causing the debounce window to
    silently collapse to the ~50ms poll interval once the watcher had run
    longer than `window_ms`. Fixed by sharing one `Instant` origin between
    the callback and the debounce thread. Added a test that runs the
    watcher past its own debounce window, then fires a rapid multi-tick
    burst of writes to the same file, and asserts they still coalesce into
    exactly one enqueued indexing job.
  - `Debouncer::drain_ready`'s comparison logic itself was not touched —
    only how the two timestamps it compares are computed.
- **[Audit pass, prior entry]** — Full source-level engineering audit
  performed across all crates and the frontend. No code changed. This
  README rewritten to replace the previous inaccurate "skeleton only, no
  business logic" framing with a verified, itemized status. Findings:
  DOCX parser and folder-watcher debounce bugs confirmed still present from
  a prior internal audit; the prior audit's prompt-dropping bug confirmed
  fixed; five frontend views confirmed built but unrouted; Concept Graph
  confirmed to have zero extraction logic (self-disclosed in source);
  vector-store architecture-contract deviation identified.

---

## Layout

```
/app            -- frontend (React/TS/Tailwind) + src-tauri (Rust workspace)
/scripts        -- dev/build/check scripts
/.github/workflows -- CI
/docs           -- prior audit reports
```

## Getting started

```
cd app
npm install
npm run tauri -- dev      # or: ../scripts/dev.sh
```

See `scripts/check.sh` for the full lint/format/typecheck/clippy/test suite
also run in CI (`.github/workflows/ci.yml`).