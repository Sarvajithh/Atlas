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

**Estimated overall completion toward the Atlas v1.0 vision: ~62%.**
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

### Student Memory
- Annotations, bookmarks, chat history, and progress/analytics repositories
  (`atlas-memory`), following the architecture contract's Student Memory
  non-destructive-deletion guarantee

### Learning
- **Quiz / Flashcard / Revision Planner generation depth** — `generate_quiz`,
  `generate_flashcards`, and `generate_revision_plan` (`atlas-models`) now
  produce typed, validated structured output (`QuizQuestion`, `Flashcard`,
  `RevisionPlanItem` in `atlas-types`) instead of free-text wrappers. The
  model is prompted (new `PromptBuilder::build_quiz_prompt`/
  `build_flashcard_prompt`/`build_revision_plan_prompt` templates,
  settings-overridable like every other prompt in this codebase) to return
  JSON; a new `study_output` module (`atlas-models`) parses and validates
  it (options must contain the stated `correct_answer`, no empty fields,
  etc.), retrying once with a corrective instruction on either malformed
  JSON or JSON that fails validation, and failing `Recoverable` (not
  panicking) if the retry also fails. Still routed through
  `EnginePool::run_role` — no bypass of engine dispatch.
- **Structured persistence** — `Quiz`/`FlashcardSet`/`RevisionPlan` records
  are persisted via a new `StudyRepository` trait (`atlas-memory`,
  implemented by `SqliteStudyRepository` in `atlas-db`), tagged by
  workspace/document/topic, subject to the same Student Memory
  non-destructive-deletion guarantee as annotations/bookmarks. New additive
  migration `0017_create_quiz_flashcard_revision_plan` (see Changelog).
- **Real weak-topic detection** — `AnalyticsRepository` gained
  `record_quiz_answer`/`list_weak_topics`: an incrementally-updated,
  real correctness aggregate per topic tag (`quiz_topic_stats` table),
  not something re-derived by the LLM on every read. The Revision Planner
  consumes this computed aggregate as structured prompt input (rather than
  operating blind or a caller-supplied concept-id list).
- **IPC** — `assistant_quiz`/`assistant_flashcards`/`assistant_revision_plan`
  now return the typed, persisted record; new `assistant_get_quiz`,
  `assistant_list_quizzes`, `assistant_list_flashcard_sets`,
  `assistant_list_revision_plans`, `assistant_submit_quiz_answer`, and
  `memory_list_weak_topics` commands.
- **Frontend** — `QuizExamMode` and `MemoryAnalyticsView` are wired to real,
  typed IPC data end-to-end (topic input → generated quiz → answer
  submission → score; weak-topic bar list; revision-plan generation and
  display). No mock data. `npm test` covers both with real IPC-mocked
  component tests.
- Not yet done: weak topics are tagged by free-text topic string, not
  `ConceptNodeId` (the Concept Graph crate produces zero nodes today, so
  this was a deliberate decoupling — see code comment in
  `atlas-types/src/memory.rs`), meaning `LearningProgress`'s per-concept
  mastery/weakness tracking (§19/§33.17-18) is unaffected by this
  milestone — quiz results update the new topic-tag aggregate only; no UI
  for browsing/deleting old quizzes beyond the simple list; no
  bulk/scheduled revision-plan regeneration.

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
  Document View, Research Mode, and Quiz/Exam Mode) — see Changelog.
  `QuizExamMode` and `MemoryAnalyticsView` are now wired to real backend
  data (see "Learning" above); `ConceptGraphView` and `ResearchMode` still
  render an honest empty/loading state pending their own backend surfaces
  (Concept Graph extraction, Research Mode's cross-document linking) --
  each remains tracked below under its own item, not silently marked done
  here.

---

## Remaining Atlas v1.0 Work

*(Copied forward from the full engineering audit; nothing removed except
items independently re-verified as already done above.)*

### High priority
- **Concept Graph construction/extraction logic** — the crate currently
  contains only repository interfaces and injected-dependency scaffolding;
  its own code comment explicitly states extraction logic is "deferred to
  a future milestone." No nodes/edges are ever produced today.

### Medium priority
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
| Learning | ~55% |
| Research | ~10% |
| UI (shell + navigation) | ~60% |
| Performance | ~45% |
| **Overall** | **~62%** |

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

- **[Learning subsystem: Quiz/Flashcard/Revision Planner structured output,
  this entry]** — Implemented the Learning subsystem milestone: typed,
  validated quiz/flashcard generation, real weak-topic-detection
  computation, and a revision planner that consumes it, wired end-to-end
  to `QuizExamMode`/`MemoryAnalyticsView`.
  - `atlas-types::memory`: added `QuizQuestion`, `Quiz`, `Flashcard`,
    `FlashcardSet`, `WeakTopic`, `RevisionPlanItem`, `RevisionPlan`; new
    `QuizId`/`FlashcardSetId`/`RevisionPlanId` newtypes. `WeakTopic` is
    keyed by a free-text `topic: String` tag rather than `ConceptNodeId`
    (see comment in that file for why -- the Concept Graph crate produces
    zero nodes today).
  - New `atlas-models::study_output` module: `parse_quiz_response`/
    `parse_flashcard_response`/`parse_revision_plan_response` parse and
    *validate* the model's JSON (strips an optional markdown code fence
    first; checks non-empty fields, `correct_answer` must be one of its
    own `options`, etc.), returning a `Recoverable` `AppError` on either
    malformed JSON or JSON that parses but fails validation -- never a
    panic, never a silent pass-through of unusable data.
  - `atlas-models::engines`: `generate_quiz`/`generate_flashcards`/
    `generate_revision_plan` now return the typed, validated result
    (previously raw `EngineOutput`/`String`), via a shared
    `generate_structured` helper that retries exactly once with a
    corrective prompt on a parse/validation failure before giving up --
    still routed through `EnginePool::run_role`, no change to engine
    dispatch.
  - `atlas-models::prompt_builder`: added `build_quiz_prompt`/
    `build_flashcard_prompt`/`build_revision_plan_prompt`, each
    settings-overridable (`learning.quiz_prompt_template`, etc.) following
    the exact `system_prompt` fallback pattern. The revision-plan prompt
    takes computed `&[WeakTopic]` data instead of retrieved context.
  - `atlas-memory`: new `StudyRepository` trait (Quiz/FlashcardSet/
    RevisionPlan persistence -- kept separate from
    `LearningProgressRepository` since these are topic-tagged generated
    artifacts, not `ConceptNodeId`-keyed mastery tracking, which this
    milestone does not touch). `AnalyticsRepository` gained
    `record_quiz_answer`/`list_weak_topics` (a real, incrementally-updated
    correctness aggregate, ordered weakest-first). `MemoryEngine` extended
    with a `study()` accessor. In-memory test doubles added for both.
  - `atlas-db`: new additive migration `0017_create_quiz_flashcard_
    revision_plan` -- `quizzes`, `flashcard_sets`, `revision_plans` (JSON-
    payload storage, workspace/document/topic-tagged like the rest of the
    schema) and `quiz_topic_stats` (the weak-topic aggregate table,
    upserted via `ON CONFLICT ... DO UPDATE SET correct_count =
    correct_count + excluded.correct_count`). No existing table touched.
    `SqliteStudyRepository`/extended `SqliteAnalyticsRepository`
    implement the above.
  - `atlas-core::facade`: `AppFacade::quiz`/`flashcards` now do
    retrieval+context assembly (same pattern `chat` uses, factored into a
    new `assemble_context_for` helper) then call the structured
    generation functions and persist the result via `StudyRepository`,
    returning the typed, persisted record. `revision_plan` now takes only
    a `workspace_id` (no caller-supplied concept-id list) and consumes
    `AnalyticsRepository::list_weak_topics` instead. Added
    `record_quiz_answer`/`list_weak_topics`/`get_quiz`/`list_quizzes`/
    `list_flashcard_sets`/`list_revision_plans`.
  - `app-tauri/src/commands/assistant.rs`+`memory.rs`: `assistant_quiz`/
    `assistant_flashcards`/`assistant_revision_plan` now return the typed
    record (previously a free-text `GeneratedContent { content: String,
    .. }`, which the frontend had no reliable way to render as an
    interactive exam). New `assistant_get_quiz`, `assistant_list_quizzes`,
    `assistant_list_flashcard_sets`, `assistant_list_revision_plans`,
    `assistant_submit_quiz_answer`, `memory_list_weak_topics` commands,
    registered in `main.rs`.
  - Frontend: `ipc/types.ts` gained the typed mirrors (`Quiz`,
    `QuizQuestion`, `Flashcard`, `FlashcardSet`, `WeakTopic`,
    `RevisionPlan`, `RevisionPlanItem`), replacing the old free-text
    `GeneratedContent`. `ipc/assistant.ts`/`ipc/memory.ts` updated to
    match. `QuizExamMode` is now a real quiz flow: generate-by-topic, an
    interactive answer picker, per-question answer submission (feeding
    the weak-topic aggregate), and a resumable quiz list -- no mock data.
    `MemoryAnalyticsView` renders the real weak-topic aggregate as a
    ranked list with accuracy bars, and a revision-plan generator/viewer.
    Added `views/__tests__/QuizExamMode.test.tsx` and
    `MemoryAnalyticsView.test.tsx` (8 tests total, real IPC-mocked
    component tests, no mock data baked into the components themselves).
  - **Verification**: `atlas-types`, `atlas-models`, `atlas-memory`,
    `atlas-db`, and `atlas-core` were all built and tested for real in
    this sandbox (`cargo test`, apt-installed cargo/rustc 1.75, in a
    disposable copy of the workspace with `Cargo.lock` deleted and a
    handful of transitive deps pinned to pre-edition2024 versions --
    `time`, `idna_adapter`, `rayon`/`rayon-core` -- the original
    `Cargo.lock` was never touched). 93/93 `atlas-models`, 62/62
    `atlas-db`, 10/10 `atlas-memory`, 29/29 `atlas-core` tests pass, 0
    regressions. **`app-tauri` itself could not be verified** -- it pulls
    in `plist` (a `tauri` dependency) which requires `time >=0.3.47`,
    which requires edition2024, a hard wall this sandbox's toolchain can't
    cross (installing a newer toolchain via `rustup` isn't possible under
    this environment's network allowlist). The `app-tauri` command changes
    were reviewed carefully by hand against the now-verified `AppFacade`
    signatures but that is not a substitute for an actual compile --
    flagging this honestly. Frontend: `npx tsc --noEmit` clean, `npx
    vitest run` 46/46 passing (38 pre-existing + 8 new), no regressions.
  - **Not done in this pass**: `LearningProgress`/`ConceptNodeId`-keyed
    mastery tracking (§19/§33.17-18) is untouched -- quiz results feed the
    new topic-tag aggregate, not per-concept mastery/weakness scores,
    since nothing currently populates `ConceptNodeId`s to key against (see
    Concept Graph, still 0% -- "High priority" above). This is a real gap,
    not silently implied fixed by the Learning-subsystem completion bump.

- **[Global Search]** — Implemented §9's Global Search: hybrid
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