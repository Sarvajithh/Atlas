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

**Estimated overall completion toward the Atlas v1.0 vision: ~57%.**
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

### Concept Graph
- **Extraction is real, not scaffolding.** `atlas-graph::extraction`
  (`ConceptExtractor`) runs a Reasoning-role prompt over each freshly
  (re)indexed document's chunk text, parses a small structured JSON shape
  (`{"concepts": [...], "relations": [...]}`), and persists it through the
  existing `GraphRepository` (`SqliteGraphRepository`, §33.5/§33.6 tables
  that already existed). No new inference stack — reuses the same
  `EnginePool`/`EngineRole::Reasoning` role every other AI feature runs
  through (`atlas-core::graph_extraction::EnginePoolConceptExtractor`).
- **Dedup, not duplication.** Re-extracting the same or overlapping
  content reuses existing nodes by case-insensitive label match
  (`GraphRepository::find_node_by_label`) and skips already-present edges
  (`GraphRepository::find_edge`) rather than growing the graph unbounded
  on every re-index.
- **Wired into the real indexing path**, not a separate manual trigger:
  `IndexingWorker` (§21) runs extraction immediately after a document
  actually indexes (`IndexOutcome::Indexed`, never on `Skipped`/unchanged
  files), via the additive `IndexingWorker::with_concept_extractor`
  builder; `AppFacade::new` constructs and attaches it in
  `start_indexing_worker`, so this happens automatically for every linked
  workspace, not just in tests.
- Extraction failures (model unreachable, malformed JSON) are logged and
  swallowed — they never fail the indexing job itself (§45.1/§45.2
  Recoverable), matching every other best-effort enrichment step in the
  pipeline.
- Not yet done: no UI trigger to force re-extraction on demand;
  extraction runs per-document, not incrementally deduped within a single
  very long document beyond the ~12k-character text cap sent to the
  model. (Frontend now does read real graph data — see Research Mode,
  below.)

### Research Mode
- **Literature review & paper comparison are real**, not scaffolding:
  `Retriever::retrieve_multi_workspace` (additive; existing single-
  workspace `retrieve` is completely unchanged) runs the same hybrid
  retrieval per requested workspace and merges results by score —
  `ContextBuilder::assemble` itself needed no changes, since chunk ids
  are globally unique across the shared SQLite database, so a merged
  multi-workspace pool flows through it safely. `PromptBuilder::
  build_research` labels every numbered context block with its actual
  source (`"[2] (source: Workspace B / paper2.pdf)"`), with two system-
  prompt framings (`ResearchPromptMode::LiteratureReview` synthesizes
  across sources; `PaperComparison` explicitly structures the answer
  around agreement/disagreement/gaps) — both routed through
  `EngineRole::Reasoning`, same role Concept Extraction uses (§14.1's
  role table isn't extended). IPC: `rag.researchQuery`
  (`commands::rag::rag_research_query`).
- **Citation Graph is real**, not a mock relationship list: a new
  `concept_node_sources` join table (migration `0017`) records which
  document(s) each Concept Graph node was actually extracted from
  (populated by `ConceptExtractor`, which now takes a `document_id`);
  `atlas_graph::citation_graph::list_cross_document_edges` finds real
  `concept_edges` rows whose endpoints are, between them, sourced from
  more than one document — a within-one-document relation is
  intentionally excluded, since that isn't a cross-document citation.
  IPC: `graph.citationGraph` (`commands::graph::graph_citation_graph`).
- **Frontend is wired to real data**, not placeholders:
  `ResearchMode.tsx` composes `ResearchQueryPanel.tsx` (workspace multi-
  select + literature-review/paper-comparison toggle + real
  `rag.researchQuery` call + rendered citations) and
  `CitationGraphView.tsx` (real `graph.citationGraph` call, honest empty
  state when no cross-document edges exist yet — never fabricated). `npx
  tsc --noEmit` passes clean; 7 new tests in
  `ResearchMode.test.tsx` cover the empty-workspaces state, both query
  modes actually sending the right `mode` over IPC, real citations
  rendering, the citation graph's populated and empty states, the
  Timeline deferred-state message, and an IPC-failure error state — all
  passing.
- **Timeline is explicitly deferred, not silently skipped**:
  `DocumentRecord` (§33.2) has no publication/authored-date field, only
  filesystem `mtime` — surfacing that as a "timeline" would be actively
  misleading (a re-saved older document would sort as recent). Needs
  parser-level date extraction, out of this phase's scope; the
  `ResearchMode.tsx` Timeline tab says this plainly rather than showing
  empty or fake content.
- Not yet done: no visual node-link graph rendering for Citation Graph
  (currently a grouped list — the query being real and correct was this
  phase's focus, not graph layout); no combined "search across
  literature review + citation graph" view; Paper Comparison and
  Literature Review currently differ only in system-prompt framing, not
  in a structurally different UI (e.g. side-by-side per-source columns).

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
  `QuizExamMode`, `MemoryAnalyticsView`, and `DocumentView` are routed and
  reachable from the running shell (`ActivityRail` for Concept Graph and
  Memory & Analytics; a workspace-scoped mode switcher in `TopNav` for
  Document View, Research Mode, and Quiz/Exam Mode) — see Changelog.
- **Document View, Concept Graph View, Quiz/Exam Mode, and Memory &
  Analytics View now render real content, fixed this phase.** Until this
  fix, all four were routed to a bare, empty component
  (`return <section aria-label="..." />;`) with no data fetching, no IPC
  call, and no loading/empty/error state — a silent blank pane every
  time, regardless of whether the workspace had data. **This was not a
  regression: these four views never rendered real data at any point in
  this repository's history** — the "wiring only, feature logic tracked
  separately" framing directly above (and the equivalent claims in the
  Phase 4/5 reports that introduced these views) undersold what was
  actually shipped, which was zero logic, not partial logic. Research
  Mode (`ResearchMode.tsx`) was the only one of the five routed views
  that was ever real, which is why it alone survived a running-app
  screenshot check unchanged.
  - `ConceptGraphView` now calls real `graph.get` per workspace, with a
    workspace picker and honest loading/empty/error states — same data
    source `MemoryAnalyticsView` below reuses, same one already proven
    real for Research Mode's Citation Graph.
  - `MemoryAnalyticsView` lists a workspace's real concept nodes
    (`graph.get`) and fetches real per-concept mastery/weakness data
    (`memory.getWeaknesses`). There is no aggregate "all progress rows
    for a workspace" backend command yet, so this view is per-concept,
    not a single dashboard query — a concept with no recorded attempts
    shows "Not yet reviewed", never a fabricated score.
  - `QuizExamMode` now calls the real `assistant.quiz` /
    `assistant.flashcards` IPC (`app-tauri/src/commands/assistant.rs`,
    `AppFacade::quiz`/`AppFacade::flashcards`) — that backend logic and
    its frontend IPC wrapper (`ipc/assistant.ts`) already existed and
    already worked; nothing in the UI layer had ever called them.
  - `DocumentView` composes the same real `DocumentExplorer` and
    `DocumentViewer` components `WorkspaceDetail`'s tab system already
    used — those were real and working, just unreachable from this
    view's own route (only `WorkspaceDetail` could open a document tab).
    This route now owns that trigger itself: pick a workspace, open a
    document from the explorer, it renders in the same real viewer.
  - Node-link graph layout for Concept Graph, a true aggregate analytics
    dashboard for Memory & Analytics, and a structured quiz/flashcard
    UI (vs. today's plain-text model output) remain open follow-ups —
    tracked below, not silently implied done by this fix.

---

## Remaining Atlas v1.0 Work

*(Copied forward from the full engineering audit; nothing removed except
items independently re-verified as already done above.)*

### Medium priority
- **Quiz / Flashcard / Revision Planner generation depth** — currently
  one-line wrappers around a generic LLM call with no structured output
  schema, no persisted structured records, and no real weak-topic-detection
  computation behind them.
- **Research Mode: Timeline** — explicitly deferred (not silently
  skipped): `DocumentRecord` (§33.2) carries only a filesystem `mtime`,
  not a publication/authored date, so there's no genuine chronological
  metadata to surface. Needs parser-level date extraction (a PDF's
  metadata date, or a document's own "Published on ..." text) — deeper
  parser work than the Research Mode phase's scope. Flagged in
  `ResearchMode.tsx`'s own doc comment and its Timeline tab, not hidden.
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
- ~~Stray untracked-by-the-build directory `app/app/`~~ — **resolved this
  phase**: it contained a second, more complete copy of several backend
  crates (e.g. an `atlas-graph`/`atlas-core` variant with real extraction
  logic, found during the Concept Graph extraction phase) and several
  frontend test files (`app/app/src/views/__tests__/{ConceptGraphView,
  QuizExamMode,MemoryAnalyticsView}.test.tsx`, found during the Research
  Mode phase while verifying `npx vitest run`). Neither half was wired
  into the real build: the Rust side wasn't a member of `app/src-tauri/
  Cargo.toml`'s workspace, and the frontend side wasn't under `app/src`,
  so its test files imported the *real* (still-stub) components via the
  `@` alias and failed rather than testing the shadow implementation they
  were presumably written against — `npx vitest run` with no exclusion
  used to report 3 failing test files unrelated to whatever phase was
  running. Deleted outright (`git rm -r app/app`) rather than integrated,
  since neither half was ever referenced by any real config, and its
  presence had already caused two phases in a row to have to re-diagnose
  the same false failures. `npx vitest run` now passes clean (9/9 test
  files, 45/45 tests) with no exclusion needed.

---

## Implementation Status by Subsystem

| Subsystem | Estimated completion |
|---|---|
| Workspaces | ~68% |
| Document System | ~57% |
| Knowledge Ingestion | ~60% |
| AI System (core chat) | ~65% |
| RAG | ~60% |
| Learning | ~28% |
| Research | ~10% |
| UI (shell + navigation) | ~60% |
| Performance | ~45% |
| **Overall** | **~57%** |

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

- **[Fix pass, this entry: blank-view regression, not new feature work]**
  — Diagnosed and fixed the four blank routed views (Document View,
  Quiz/Exam Mode, Concept Graph View, Memory & Analytics View). Root
  cause was shared for three of the four:
  `ConceptGraphView.tsx`/`QuizExamMode.tsx`/`MemoryAnalyticsView.tsx`
  were literal one-line stub components (`return <section aria-label=
  "..." />;`) with no data fetching, IPC call, or state of any kind —
  this was **never working**, not a regression from a previously-passing
  state, despite this file's own prior "manual trace: renders real data"
  language for the phases that introduced them. `DocumentView.tsx` was
  also a stub, but for a different reason: the real viewer (`DocumentViewer`,
  `DocumentExplorer`) already existed and worked inside `WorkspaceDetail`'s
  tab system — it just had no path to the `document-view` route.
  Research Mode was the only one of the five routed views ever real,
  which is why it alone survived a running-app check.
  - `ConceptGraphView` → real `graph.get` per workspace, honest
    loading/empty/error states.
  - `MemoryAnalyticsView` → real `graph.get` + per-concept
    `memory.getWeaknesses`; no aggregate progress query exists in the
    backend yet, so this is a per-concept list, not a single dashboard
    call, and shows "Not yet reviewed" rather than a fabricated score.
  - `QuizExamMode` → wired to the already-real, already-working
    `assistant.quiz`/`assistant.flashcards` IPC, which nothing in the UI
    had ever called.
  - `DocumentView` → composes the existing real `DocumentExplorer` +
    `DocumentViewer` + document-tab store, giving this route its own
    trigger to open a document instead of relying on `WorkspaceDetail`.
  - The `app/app/` cleanup this fix's task brief asked for was already
    done — verified via `git log --diff-filter=D` that the Phase 6
    commit itself deleted that directory; the current tree has no
    `app/app/` and needed no further action.
  - Added `src/test-setup.ts` (a `DOMMatrix` shim for `pdfjs-dist` under
    jsdom, a pre-existing test-environment gap, not new production
    logic) and wired it into `vitest.config.ts`'s `setupFiles`, needed to
    get `DocumentView`'s new tests running since it now pulls in
    `PdfViewer` transitively.
  - `npx tsc --noEmit` clean. `npx vitest run` (no exclusion) passes:
    13/13 test files, 56/56 tests (up from 9/9, 45/45 — 4 new test files
    for the four fixed views). `ResearchMode.tsx` and its tests
    untouched, confirmed unchanged by re-running its suite.
- **[`app/app/` cleanup, this entry]** — Deleted the stray,
  untracked-by-the-build `app/app/` directory (35 tracked files: a
  partial duplicate/shadow copy of several backend crates and 3 frontend
  test files), flagged across the two previous phases as a source of
  false test failures and dead weight. `git rm -r app/app`. Confirmed
  `npx vitest run` (no exclusion) now passes clean: 9/9 test files, 45/45
  tests. See Known Limitations above for what it contained and why it
  wasn't simply integrated instead.
- **[Research Mode, this entry]** — Implemented Literature Review, Paper
  Comparison, and Citation Graph on top of the Concept Graph extraction
  landed in the previous phase; Timeline explicitly deferred (see
  Completed Features > Research Mode and Remaining Work above for why).
  - `atlas-models::retriever::Retriever::retrieve_multi_workspace` (new,
    additive): runs the existing single-workspace `retrieve` once per
    requested workspace and merges by score. `retrieve` itself is
    unchanged — every query still keyword/vector-searches exactly one
    workspace at a time, preserving per-workspace data scoping; only the
    merge is new. `ContextBuilder::assemble` needed **no code changes**
    to consume the merged pool, since chunk ids are already globally
    unique across the app's single shared SQLite database.
  - `atlas-models::prompt_builder::PromptBuilder::build_research` (new):
    Research Mode's variant of `build`, with `ResearchPromptMode::{
    LiteratureReview, PaperComparison}` selecting between a synthesize-
    across-sources system prompt and an explicit-comparison one. Each
    numbered context block is labeled with its real source (`"[2]
    (source: Workspace B / paper2.pdf)"` — resolved via a new
    `AppFacade::research_source_labels` helper), not left anonymous.
  - `atlas-db` migration `0017_create_concept_node_sources` (new table,
    additive — `concept_nodes`/`concept_edges` themselves untouched):
    records which document(s) each concept node was actually extracted
    from. `atlas_graph::repository::GraphRepository` gained
    `record_node_source`/`list_source_documents`; `ConceptExtractor::
    extract_and_store` (from the previous phase) now takes a
    `document_id` and records provenance for every node it creates or
    reuses — its one real caller (`atlas-core::worker`) and all its unit
    tests were updated to pass the document's actual id.
  - `atlas-graph::citation_graph::list_cross_document_edges` (new
    module): finds real `concept_edges` rows whose endpoints are,
    between them, sourced from more than one document (via the new
    provenance table) — a within-one-document relation is excluded on
    purpose, matching "citation graph" rather than the full Concept
    Graph. No mock/fabricated relationships: every edge and every
    `source_documents` entry traces to a real stored row.
  - IPC: `commands::rag::rag_research_query` (`rag.researchQuery`) and
    `commands::graph::graph_citation_graph` (`graph.citationGraph`),
    registered in `main.rs`. `AppFacade::research_query` deliberately
    bypasses the Intent/Scheduler pipeline `chat`/`chat_stream` use
    (that pipeline's routing is scoped to one workspace's `Intent`,
    which is on this and the prior phase's MUST-NOT-change list) —
    Research Mode calls Retriever/ContextBuilder/PromptBuilder/
    EnginePool directly instead, through the same `EngineRole::
    Reasoning` role Concept Extraction already uses.
  - Frontend: `ipc/rag.ts` (new), `ipc/graph.ts` extended with
    `graphCitationGraph`, new types in `ipc/types.ts`
    (`ConceptEdge`, `CitationGraphEdge`, `SearchResult`).
    `ResearchMode.tsx` now composes real subcomponents under
    `views/research/`: `WorkspaceMultiSelect.tsx`, `ResearchQueryPanel.tsx`
    (literature review / paper comparison, real IPC round-trip, rendered
    citations), `CitationGraphView.tsx` (real IPC round-trip, honest
    empty state — "no cross-document relationships found yet", never
    fabricated content to fill the space). The Timeline tab states
    plainly why it isn't implemented rather than showing an empty or
    fake view.
  - Tests: 6 new backend unit tests (`retriever.rs`: 2 for
    `retrieve_multi_workspace`; `prompt_builder.rs`: 4 for
    `build_research`) plus 5 in `atlas-graph::citation_graph` and 3 in
    `atlas-db::graph_adapter`/`atlas-graph::testing` for the provenance
    methods (all counted in the `atlas-graph` totals below) — the
    `atlas-graph` crate's own test count is now **18 passing** (see prior
    entry's 13, plus this phase's 5 citation-graph tests). 7 new frontend
    tests in `src/views/__tests__/ResearchMode.test.tsx`, covering the
    no-workspaces state, a real literature-review round-trip with
    rendered citations, the paper-comparison mode actually sending that
    mode over IPC, the citation graph's populated and empty states, the
    Timeline deferred message, and an IPC-failure error state. **All
    frontend tests verified passing in this environment**: `npx vitest
    run --exclude "**/app/**"` — 9/9 real test files, 45/45 tests — and
    `npx tsc --noEmit` clean, both run against a real `npm install` in
    this sandbox (unlike the Rust side, Node/npm were available here).
  - **Discovery, not caused by this phase**: running the frontend suite
    without excluding anything showed 3 failing test files
    (`ConceptGraphView.test.tsx`, `QuizExamMode.test.tsx`,
    `MemoryAnalyticsView.test.tsx`). Traced this to the stray
    `app/app/` directory flagged in the previous phase's report: it
    contains its own copies of exactly these 3 test files, which import
    the real (still-stub) components via the `@` alias and so fail
    against them. Confirmed by re-running with `--exclude "**/app/**"`:
    clean, 9/9/45/45. Documented under Known Limitations above so future
    phases don't have to rediscover this; not fixed here since `app/app/`
    cleanup is outside this phase's scope.
  - **Deviation from this phase's originally-stated file scope, flagged
    rather than silently done**: the task instructions for this phase
    listed `context_builder.rs` as an allowed-to-change file
    ("extend to support cross-workspace/multi-document context
    assembly"). It ended up **not needing any changes** — the existing
    `assemble` already merges an arbitrary `Vec<SearchHit>` regardless of
    which workspace(s) produced it, since chunk ids are globally unique.
    Noted explicitly rather than silently skipping a listed file, per
    this project's own "report deviations and why" convention.
  - **Known verification limitation, same as the previous phase**: the
    Rust changes in this entry (`atlas-models`, `atlas-core`,
    `atlas-db`, `app-tauri`) are hand-reviewed against the real
    trait/struct signatures but **not compiler-verified** — this
    sandbox's only available toolchain (apt's rustc/cargo 1.75) still
    can't resolve the full workspace's dependency graph (`atlas-indexer
    -> lopdf -> time`/`idna_adapter` need edition2024). I got further
    than last time (managed to downgrade `time` itself via `cargo update
    --precise`) but hit the same wall one dependency layer deeper
    (`idna_adapter`, pulled in transitively via `ureq`'s URL-parsing
    stack) and stopped rather than continue whack-a-mole-downgrading
    unrelated dependencies with no network access to a newer toolchain.
    `atlas-graph` (the crate with the actual provenance/citation-graph
    logic) **is** compiler-verified — 18/18 tests passing. The frontend
    changes in this entry, unlike last phase, **are** fully verified
    (`tsc` + `vitest`, both real). Whoever picks this up next should run
    `cargo build && cargo test` for `atlas-models`, `atlas-core`,
    `atlas-db`, and `app-tauri` with a proper toolchain before merging.
- **[Concept Graph extraction, this entry]** — Closed the previously
  self-disclosed gap: `atlas-graph`'s `engine.rs` said extraction was
  "deferred to a future milestone" and no code path ever produced a node
  or edge from real content. It now does.
  - `atlas-graph::extraction` (new module): `ConceptExtractor` runs a
    caller-supplied `ConceptExtractionModel` (a narrow seam — this crate
    still doesn't depend on `atlas-models`, avoiding the
    `atlas-models -> atlas-indexer` cycle) over source text via
    `build_extraction_prompt`, parses a small JSON
    `{"concepts": [...], "relations": [...]}` shape, and persists it
    through `GraphRepository`. Tolerates a markdown code fence around the
    JSON if the model adds one despite being told not to. A relation
    referencing a label not present in `concepts` is skipped rather than
    fabricating a node for it (no mock/fabricated relationships).
  - `atlas-graph::repository::GraphRepository`: added
    `find_node_by_label` (case-insensitive, workspace-scoped) and
    `find_edge` (exact from/to/type) so re-extraction reuses existing
    nodes/edges instead of duplicating them on every re-index. Implemented
    for both `SqliteGraphRepository` (`atlas-db`) and the crate's own
    `InMemoryGraphRepository` test double, which was also given real
    auto-incrementing ids on insert (it previously only stored whatever id
    the caller passed in, which is fine for its original hand-authored
    fixture tests but not for a dedup workflow that needs distinct ids
    assigned by the repository itself, matching how `SqliteGraphRepository`
    already behaved via SQLite's own rowid).
  - `atlas-core::graph_extraction` (new module): `EnginePoolConceptExtractor`
    is the concrete adapter from `ConceptExtractionModel` to
    `atlas_models::EnginePool`, routed through `EngineRole::Reasoning` —
    the same "feature built on an existing role" pattern
    `atlas-models::engines` already uses for Quiz/Flashcard/Revision
    Planner, so §14.1's frozen Engine-role table isn't touched.
  - `atlas-core::worker::IndexingWorker`: added an additive
    `with_concept_extractor` builder (existing call sites/tests that never
    call it keep today's indexing-only behavior unchanged). After a
    document actually indexes (`IndexOutcome::Indexed`, never on a
    cache-hit `Skipped`), the worker loads that document's chunks (via the
    same `ChunkRepository` the indexing pipeline already populated),
    concatenates up to ~12k characters of their text, and runs extraction.
    A failure here (model unreachable, malformed JSON) is logged and
    otherwise swallowed — it never un-succeeds the already-recorded
    indexing job (§45.1/§45.2 Recoverable; extraction is a best-effort
    enrichment layered on top of indexing, not a condition of it).
  - `atlas-core::facade::AppFacade`: constructs one `ConceptExtractor`
    (backed by the real `SqliteGraphRepository` and the real
    `EnginePool`) in `new`, and attaches it in `start_indexing_worker` —
    so this runs automatically for every linked workspace, not just in
    tests.
  - Tests: 8 new unit tests in `atlas-graph::extraction` (well-formed
    extraction, code-fence stripping, label-based dedup on re-extraction,
    duplicate-edge skipping, orphan-relation skipping, malformed-JSON as a
    recoverable error not a panic, empty-text short-circuit that never
    even calls the model, prompt-content assertion) plus one new
    `atlas-core::worker` end-to-end test
    (`worker_runs_concept_extraction_after_a_real_index`) that drives a
    real `IndexingWorker` against a real temp-directory file through to
    persisted graph nodes, not just the extractor in isolation.
  - **Deviation from this phase's stated scope, flagged rather than
    silently done**: the task instructions for this phase named
    `context_builder.rs`, `retriever.rs`, `atlas-graph`'s citation/
    relation *queries*, `commands/rag.rs`/`graph.rs`, and
    `ResearchMode.tsx` as the allowed-to-change surface, for building
    Research Mode's literature review/citation graph/timeline UI on top
    of an already-populated Concept Graph. Investigation found that
    prerequisite false — Concept Graph extraction itself was the missing
    piece, not present anywhere in the codebase, contradicting this
    phase's own stated dependency ("depends on Phase 5's Concept Graph
    being in place"). Per this phase's own instruction to "stop and
    report that dependency gap rather than building on sand," Research
    Mode's UI/IPC layer was intentionally *not* built this pass; this
    entry lands the actual missing dependency (extraction) instead so a
    subsequent Research Mode phase has real data to build on.
  - **Known verification limitation**: I could only get a full `cargo
    build`/`cargo test` for the crate I added the core logic to,
    `atlas-graph`, run in this environment (13/13 tests passing, using
    apt's rustc/cargo 1.75.0 — the only Rust toolchain installable here).
    The rest of the workspace (`atlas-db`, `atlas-core`, `app-tauri`)
    could not be compiled in this sandbox: the committed `Cargo.lock` is
    lockfile-format-v4, which requires cargo ≥1.78, and regenerating a
    fresh lock hits a transitive dependency (`atlas-indexer -> lopdf ->
    time`) whose latest version requires edition2024, which 1.75 can't
    parse even to select an older compatible version without a working
    lockfile already pinning one. rustup's install domain isn't in this
    environment's network allowlist, so upgrading wasn't possible either.
    The `atlas-core`/`atlas-db` changes were written and hand-reviewed
    carefully against the real trait/struct signatures in the codebase
    (not guessed), but are **not yet compiler-verified** the way the
    project's own acceptance criteria (`cargo build`/`cargo test` for
    touched crates) require. Whoever picks this up next should run
    `cargo build && cargo test` for `atlas-core`, `atlas-db`, and
    `app-tauri` with a proper toolchain before merging, and fix anything
    a real compiler catches that this review didn't.
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