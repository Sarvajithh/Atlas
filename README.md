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

**Estimated overall completion toward the Atlas v1.0 vision: ~35–40%.**
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
  `atlas-core::worker`) — **functional, with a known debounce bug, see
  Technical Debt**
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
- DOCX parsing — **present but only for uncompressed (STORED) ZIP entries;
  see Known Limitations, this fails on real Word/LibreOffice output**

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

---

## Remaining Atlas v1.0 Work

*(Copied forward from the full engineering audit; nothing removed except
items independently re-verified as already done above.)*

### High priority
- **Frontend navigation wiring** — `ConceptGraphView`, `ResearchMode`,
  `QuizExamMode`, `MemoryAnalyticsView`, and a dedicated `DocumentView` all
  exist as built components but are not reachable from the running app;
  `App.tsx`'s router only ever renders Workspace Home, Workspace Detail, or
  Settings.
- **DOCX parser correctness** — cannot read real (DEFLATE-compressed)
  Word/LibreOffice/Google-Docs-exported files; degrades to an empty block.
- **Global Search** — required by the architecture contract's navigation
  flow (§9); no unified hybrid-search IPC command or frontend surface
  exists yet, despite the underlying retrieval/reranking machinery being
  reusable for it.
- **Concept Graph construction/extraction logic** — the crate currently
  contains only repository interfaces and injected-dependency scaffolding;
  its own code comment explicitly states extraction logic is "deferred to
  a future milestone." No nodes/edges are ever produced today.

### Medium priority
- **Folder watcher debounce bug** — the debounce window collapses to
  roughly the ~50ms poll interval after the watcher's first ~500ms of
  uptime, due to two different clock references being compared; defeats
  real-world debouncing of rapid-save bursts.
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

- **DOCX parser** silently produces an empty result for real-world Word
  files (see above); this is a correctness defect, not a missing feature.
- **Folder watcher debounce** degrades after startup (see above);
  functional but not reliable under rapid file-change bursts.
- **Reranker is a lightweight heuristic** (term overlap + phrase bonus),
  not a cross-encoder model — documented as an intentional scope choice in
  the source, not a hidden gap.
- **Test coverage gap pattern**: prior internal audits of this repository
  identified specific reasons existing unit tests didn't catch the DOCX and
  watcher bugs above (fixtures that bypass the real compressed/ZIP path;
  timing-independent debounce tests). These gap-closing tests have not yet
  been added even where the underlying bug (dropped user query from
  prompts) was separately fixed — any future fix to the DOCX/watcher bugs
  should add tests of the same shape the prior audit specified, not just
  fixture-level tests that would pass either way.
- **Architecture contract deviation**: custom vector store instead of the
  mandated Qdrant/LanceDB (§5 of `app/docs/README.md`) — unresolved,
  unamended as of this writing.
- Two internal audit artifacts (`SUMMARY.md`, `docs/fix7_audit_report.md`,
  `CHANGES.diff`) exist at/near the repository root from prior fix passes;
  useful history, but should eventually be consolidated into a single
  changelog rather than left as standalone documents.

---

## Implementation Status by Subsystem

| Subsystem | Estimated completion |
|---|---|
| Workspaces | ~65% |
| Document System | ~50% |
| Knowledge Ingestion | ~60% |
| AI System (core chat) | ~65% |
| RAG | ~55% |
| Learning | ~20% |
| Research | ~10% |
| UI (shell + navigation) | ~40% |
| Performance | ~45% |
| **Overall** | **~35–40%** |

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

- **[Audit pass, this entry]** — Full source-level engineering audit
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