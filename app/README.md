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

**Estimated overall completion toward the Atlas v1.0 vision: ~59%.**
See "Completed Features," "Remaining Atlas v1.0 Work," and "Known
Limitations & Technical Debt" below for the itemized breakdown this
estimate is based on. (Phase 5's task brief suggested updating this to
~75% after Concept Graph extraction landed; that jump felt too large for
one subsystem against the audit's own per-area breakdown below, so this
estimate instead raises Learning from ~20% to ~35% and nudges the overall
figure accordingly — flagged here rather than adopted silently, per this
file's own "verified against actual source, not inferred" standard.)

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
- **Global Search** (§9): hybrid keyword+vector search across the active
  workspace or all workspaces, reusing the existing `Retriever` and
  `Reranker` as-is (`AppFacade::search_global`), exposed via the
  `search_global` IPC command and reachable from the running shell (a
  discoverable "Search everything…" entry in `TopNav`, `Ctrl/Cmd+K`) — see
  Changelog

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

### Concept Graph
- Extraction pipeline (Phase 5): after a document finishes indexing with
  at least one chunk, the Indexing Worker enqueues an `extract_concepts`
  job (`atlas-indexer::job_queue`) that prompts the `EngineRole::Reasoning`
  model for structured JSON (concept names + relations), retries once on
  an unparseable response, and merges the result into the workspace's
  graph (`atlas-graph::GraphEngine::extract_for_document`)
- Cross-document dedup within a workspace by normalized (trimmed,
  lowercased) concept label, so re-indexing a second document that
  mentions an already-known concept reuses its node rather than creating a
  disconnected duplicate
- Edge dedup on `(from, to, relation_type)`, so rebuilding a workspace's
  graph does not grow edges unboundedly
- Regenerable-cache semantics preserved: extraction reads/writes only
  `GraphRepository` (SQLite-backed `graph_adapter`), never touches Student
  Memory tables — deleting a workspace's source documents does not delete
  Student Memory (§7.2)
- `ConceptGraphView` renders real nodes and their outgoing relations,
  filterable by the active workspace, backed by two new read-only IPC
  commands (`graph_get_edges`, `graph_get_concept_detail`) alongside the
  existing `graph_get`
- Not done: linking Phase 4's free-text weak-topic tags to `ConceptNodeId`
  (see "Remaining Atlas v1.0 Work" — deliberately deferred, not attempted
  partially)

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
- ~~**Concept Graph construction/extraction logic**~~ — **done, Phase 5**:
  LLM-prompted extraction over newly-embedded chunks (`atlas-graph::engine`),
  normalized-label dedup across documents within a workspace, persisted via
  `GraphRepository`, run as an async `extract_concepts` job appended after
  indexing succeeds (`atlas-core::worker`) — see Changelog.
- **Weak-topic detection → `ConceptNodeId` linkage** — Phase 4's weak-topic
  tracking still keys by free-text topic tag, not by `ConceptNodeId`, even
  though the Concept Graph now produces real nodes. This was explicitly
  scoped as an optional, non-blocking enhancement for Phase 5 and was
  deliberately *not* attempted: a fuzzy free-text-to-node match is exactly
  the kind of thing that's easy to half-implement into a subtly wrong
  state, and Phase 4's existing free-text schema must not change as a side
  effect. Left as a named, explicit item for a future integration phase.

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
| RAG | ~60% |
| Learning | ~35% |
| Research | ~10% |
| UI (shell + navigation) | ~55% |
| Performance | ~45% |
| **Overall** | **~59%** |

---

## Configuration

- `search.default_limit` (global, integer, defaults to `20` — see
  `DEFAULT_SEARCH_LIMIT` in `atlas-core::facade`): the number of Global
  Search results returned when the `search_global` IPC command's `limit`
  argument is omitted. Follows the same settings-key-with-documented-
  fallback pattern as `assistant.system_prompt_template`.

No other new configuration surfaces beyond what already existed in
`atlas-config`/Settings have been added. See `app/docs/README.md` for the
frozen settings/configuration model.

## Benchmarks

No new benchmark numbers were produced by this pass. See `SUMMARY.md` for
the most recent prompt-quality/latency benchmark predictions, which have
not been independently re-verified against a live Ollama instance in this
environment.

## Changelog

- **[Concept Graph extraction, Phase 5, this entry]** — Implemented §20's
  Concept Graph construction/extraction logic, previously deferred
  entirely:
  - Extraction approach: `atlas-graph::GraphEngine::extract_for_document`
    prompts the `EngineRole::Reasoning` model with a structured-JSON
    contract (concept names + optional descriptions, and typed relations
    among them), parses the response, retries once on unparseable output,
    then merges into the workspace's graph — new concepts by normalized
    label are inserted, already-seen labels are reused, and
    already-present `(from, to, relation_type)` edges are skipped so
    re-running extraction doesn't duplicate edges. Kept intentionally
    decoupled from `atlas-models` via a narrow `ConceptExtractionModel`
    trait (`atlas-graph::engine`), with the concrete adapter over
    `EnginePool` living in `atlas-core::graph_extraction` — avoids a
    dependency cycle (`atlas-models` already depends on `atlas-indexer`).
  - Pipeline wiring: the Indexing Worker (`atlas-core::worker`) enqueues a
    new `extract_concepts` job (`atlas-indexer::job_queue`) after an
    `index_document` job succeeds with at least one chunk, so extraction
    genuinely runs as an async step *after* embedding, never synchronously
    inside parse→OCR→chunk→embed, and a failing/slow extraction never
    affects the document's own `Parsed`/`ParsedEmpty` status.
  - Verification method: `atlas-graph`, `atlas-indexer`, and `atlas-core`
    (which pulls in `atlas-db`, `atlas-models`, and everything else except
    `app-tauri`) were all build- and test-verified together in one
    isolated workspace on the sandbox's rustc 1.75.0 — the same technique
    used in Phases 1 and 4, with the same three `--precise` downgrades
    required for `time`/`idna_adapter`/`rayon-core` to clear the
    `edition2024` wall (no logic changes, dependency-version pins only).
    `app-tauri/src/commands/graph.rs` was hand-reviewed against the
    current `GraphRepository`/`AppFacade` signatures (not
    compiler-checked — see "Tests run" in the implementation report below
    for the full breakdown).
  - Weak-topic/`ConceptNodeId` linkage: explicitly deferred, not attempted
    (see "Remaining Atlas v1.0 Work").
  - **Discrepancy noted, not silently resolved**: the phase brief for this
    work directed reuse of "the `generate_structured` retry-once-then-
    Recoverable helper established in Phase 4." No such helper exists
    anywhere in this codebase (verified via full-repo search) — Phase 4
    never introduced one. A scoped retry-once implementation was written
    directly in `atlas-graph::engine` instead of inventing a "reuse" of
    something that isn't there.
  - Frontend: `ConceptGraphView` now renders real nodes and their outgoing
    relations, filterable by the active workspace, via two new read-only
    commands (`graph_get_edges`, `graph_get_concept_detail`) added
    alongside the existing `graph_get`.

- **[Global Search, this entry]** — Implemented §9's Global Search: hybrid
  keyword+vector search across the active workspace or all workspaces.
  - `atlas-core::facade`: added `AppFacade::search_global(query,
    workspace_id: Option<WorkspaceId>, limit: Option<usize>)`. Distinct
    from the pre-existing `AppFacade::search` (which builds a RAG prompt
    for `rag_search` and was left untouched) — this returns a flat,
    display-ready ranked list instead. Reuses the existing `Retriever`
    (hybrid keyword+vector merge) and `Reranker` exactly as they already
    worked; no second scoring mechanism was introduced. `workspace_id =
    None` means "All": every `Active`/`Indexing` workspace is queried and
    the combined candidate pool is reranked together (not reranked
    per-workspace then concatenated, which would leave scores
    incomparable across workspaces). Added a `documents:
    Arc<dyn DocumentRepository>` field to `AppFacade` (cloned from the
    same repository instance `IndexingPipeline` already owns) to resolve
    each hit's document path for display.
  - `atlas-types::retrieval`: added the `GlobalSearchResult` DTO
    (document_id, workspace_id, workspace_name, chunk_id, relative_path,
    snippet, location_ref, score).
  - New `app-tauri/src/commands/search.rs` — `search_global` IPC command
    (thin passthrough to the facade, per §26/§46.4), registered in
    `main.rs`.
  - Result-limit default is settings-driven (`search.default_limit`, see
    "Configuration" above), not hardcoded, per §23.
  - Frontend: new `ipc/search.ts` (`searchGlobal`), a new
    `GlobalSearchOverlay` component (debounced query-as-you-type, a
    Workspace/All-workspaces scope toggle, selecting a result navigates to
    it as a document tab), a "Search everything…" entry point in `TopNav`
    per §8.1's Title Bar spec, and a `Ctrl/Cmd+K` shortcut wired in
    `App.tsx`. `state/store.ts` gained `isGlobalSearchOpen` /
    `setGlobalSearchOpen`, following the exact pattern already used for
    `isAssistantPanelOpen`.
  - Selecting a result reuses the existing `openTab`/`document-view`
    navigation path — `DocumentView` itself is still the pre-existing stub
    (§8.2.2, out of scope here) and does not yet read `activeDocumentId`,
    so opening a result switches to Document View and sets the right
    workspace/document context, but the reader pane itself doesn't render
    per-document content yet; that gap is tracked under Document System,
    not silently implied fixed here.
  - No mock/sample results anywhere — `GlobalSearchOverlay` only ever
    renders what `search_global` actually returns.
  - Added `components/__tests__/GlobalSearchOverlay.test.tsx` (5 tests:
    closed-by-default, no IPC call until a query is typed, real
    IPC-round-trip results rendering, result selection navigates + closes
    the overlay, default scope follows the active workspace). Full
    existing frontend suite (`npx vitest run`) passes, 38/38, no
    regressions. `npx tsc --noEmit` is clean.
  - **Not independently verified**: `cargo build`/`cargo test` for the
    Rust changes. This sandbox's available `cargo`/`rustc` (installed via
    `apt`, 1.75) is too old for this workspace's `Cargo.lock` (lockfile
    v4) and dependency tree (a transitive dependency requires
    edition2024), and installing a newer toolchain via `rustup` was not
    possible under this environment's network allowlist (rustup's
    download domains aren't on it). The original `Cargo.lock` was left
    untouched. The Rust changes were reviewed carefully by hand
    (types, imports, struct-literal field wiring, trait bounds) but that
    is not a substitute for an actual compile — flagging this honestly
    rather than claiming a build pass that didn't happen.

- **[Frontend navigation pass]** — Wired the 5 previously
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