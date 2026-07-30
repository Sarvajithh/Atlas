# Local Learning OS — Architecture Contract

> **Status:** FROZEN. This document is the single source of truth for the project.
> No architectural decision described here may be changed, redesigned, renamed, or extended without an explicit, separate instruction to amend this contract. Every future prompt, ticket, or implementation session references this file. If a future request conflicts with this document, this document wins unless the user explicitly says "amend the README."

---

## 0. How To Use This Document

- This is not a proposal. It is a constitution.
- Sections are numbered and cross-referenced. Implementation work should cite section numbers (e.g. "per §13.4 Retriever Module").
- Anything not defined here (a new engine, a new table, a new IPC command) is **out of scope** until this document is amended.
- "MUST" / "MUST NOT" / "SHOULD" follow RFC2119-style intent: MUST is non-negotiable, SHOULD is a strong default that needs a stated reason to deviate from.

---

## 1. Vision

A local-first, offline, single-user **AI Learning Operating System** for desktop. It is the environment a student or researcher lives in while studying: their linked knowledge folders, their documents, their notes, their concept graph, and one assistant that quietly does the right thing in the background.

It should feel like **VS Code, but for studying** — a workspace-and-file-centric IDE, not a chat window with a file upload button bolted on.

The product succeeds when a user can:
- Link an existing folder of PDFs/notes/books and have it indexed without ever "uploading" anything.
- Open a document and have the assistant available as context, not as the destination.
- Ask a question and have the system silently choose retrieval, tutoring, or reasoning pipelines — the user never picks a model or a mode.
- Return weeks later and have the system remember what they're weak at, what they've reviewed, and what to revise next.

## 2. Design Principles

1. **Local-first, always.** No network calls except to a locally running Ollama instance. No telemetry, no cloud sync, no login.
2. **Files are owned by the user.** The app never copies, moves, or takes ownership of source documents. It only watches and reads them.
3. **One assistant, many engines.** The user-facing surface is a single assistant. Internally, a scheduler routes work across specialized engines (§14).
4. **Document-first UI.** The primary screen real estate belongs to the document being studied. The assistant is a panel, never the main stage.
5. **Knowledge outlives files.** Deleting or moving a source file must not silently destroy derived knowledge (embeddings, notes, memory) tied to it.
6. **Explicit over implicit.** Users choose workspaces, not models; they choose folders, not pipelines. Everything else is inferred.
7. **Boring, inspectable data.** SQLite + a local vector store, not opaque blobs. Anyone should be able to open the SQLite file and understand the schema.
8. **Consistency over cleverness.** Naming, folder structure, and module boundaries defined here are stable contracts, not starting points for creative renaming.

## 3. Product Philosophy

- This is **not a chatbot**. A chat log is not the primary artifact of value; the documents and the derived knowledge are.
- This is **not a NotebookLM clone**. There is no "upload sources, get a notebook" mental model. Workspaces are living, watched folders, not static uploads.
- This is **not an AI wrapper**. The product is the workspace, the concept graph, the memory, and the study workflow. The LLM is an engine underneath it, swappable in principle, never the brand.
- The assistant is a **feature of the workspace**, not the workspace itself.

## 4. Non-Goals

Explicitly out of scope, permanently, unless this contract is amended:

- No cloud storage, cloud sync, or multi-device sync.
- No user accounts, authentication, or licensing servers.
- No subscription billing or API-key management for hosted model providers.
- No multi-user collaboration (no real-time co-editing, no sharing links).
- No mobile app.
- No support for arbitrary hosted LLM providers (OpenAI, Anthropic API, etc.) in v1. Ollama is the only inference backend (§0 tech stack is frozen).
- No "chat-first" home screen. The app must never open into a blank chat box.
- No automatic upload/copy of user files into app storage.

## 5. Tech Stack (Frozen)

| Layer | Choice |
|---|---|
| Frontend UI | React + TypeScript |
| Desktop shell | Tauri |
| Styling | TailwindCSS + shadcn/ui |
| Backend / core logic | Rust |
| Inference | Ollama (local only) |
| Relational storage | SQLite |
| Vector storage | Qdrant or LanceDB (embedded/local mode) |
| File storage | Local filesystem (read-only access to user files) |

No additional frontend framework, state library, or backend language may be introduced without an explicit justification recorded as an amendment to this section.

## 6. Workspace Philosophy

Users **never upload files**. They **link folders**.

```
Knowledge/
  Semester 5/
    Convex Optimization/
    Probability/
  Research/
  Projects/
  Books/
```

- A **Workspace** = one linked root folder (plus its subfolders).
- The app watches linked folders for changes (add/edit/delete/move) via a Folder Watcher (§21).
- Source files always remain in place, owned and controlled by the user's own filesystem tools.
- The app never writes into, moves, or copies the original files. It only reads them and writes derived data elsewhere (§7).

### 6.1 Workspace Lifecycle

```
Unlinked → Linking → Indexing (initial) → Active (incremental indexing) → Archived → (optionally) Unlinked
```

- **Unlinked**: folder is not known to the app.
- **Linking**: user selects a folder; app records the path and begins a first-time scan.
- **Indexing (initial)**: full scan, OCR/parse pass, embedding pass. Progress is visible; UI remains usable.
- **Active**: steady state. Folder Watcher applies incremental indexing on file events.
- **Archived**: user marks a workspace archived. Watching stops. Derived data (§7.2, §7.3) is retained and queryable, but no new indexing happens. Source files may even be gone (moved, on a disconnected drive, deleted) — archived workspaces MUST remain browsable via cached data.
- Deleting a workspace link removes the workspace's row and watcher registration; it does NOT delete AI Cache or Student Memory by default (§7 — destructive deletion is a separate, explicit action).

## 7. Generated Data Model

Three strictly separated categories:

### 7.1 Source Documents
The user's original files, untouched, wherever they live on disk. The app stores only a path reference + file metadata (hash, mtime, size), never the file content itself as a copy.

### 7.2 AI Cache
Derived, regenerable artifacts: OCR text, parsed structure, embeddings, chunk indexes, thumbnails, generated summaries tied to a specific document version. This is safe to delete and rebuild — it is a cache, not a record of the user's learning.

### 7.3 Student Memory
Not regenerable from source files. This is the user's learning history: quiz results, weakness scores, revision schedule state, concept mastery, annotations, bookmarks, workspace-specific chat history. Deleting the source file, or even the whole workspace link, MUST NOT delete Student Memory without a separate, explicit, confirmed destructive action.

**Rule:** Deleting a source file → AI Cache for that file becomes stale/orphaned (flagged, eventually garbage-collected). Student Memory referencing that file is preserved and re-attached if the file reappears (matched by content hash), otherwise shown as "orphaned but retained."

## 8. Complete UI Description

### 8.1 Shell Layout
Inspired by VS Code / Cursor / Linear / Obsidian.

```
┌─────────────────────────────────────────────────────────────┐
│ Title Bar (workspace switcher, search)                       │
├───────────┬─────────────────────────────────────┬────────────┤
│           │                                       │            │
│ Activity  │        Main Document Area             │  Assistant │
│  Rail     │   (PDF Viewer / Lecture Viewer /      │   Panel    │
│           │    Editor / Concept Graph / Quiz)      │  (side)    │
│  Sidebar  │                                       │            │
│ (tree)    │                                       │            │
│           │                                       │            │
├───────────┴─────────────────────────────────────┴────────────┤
│ Status Bar (indexing progress, model/engine status)           │
└─────────────────────────────────────────────────────────────┘
```

- **Activity Rail** (far left, icon strip): Workspaces, Search, Concept Graph, Memory/Analytics, Settings.
- **Sidebar**: file tree of the active workspace's linked folder(s), with indexing status badges per file.
- **Main Document Area**: the dominant surface. Always shows the current document or the current mode (Exam Mode, Research Mode, Concept Graph view).
- **Assistant Panel**: a dockable side panel (collapsible, never modal, never full-screen by default). Scoped to the current document/workspace context.
- **Status Bar**: background indexing progress, current engine activity (e.g. "Retrieving…", "Reranking…"), never blocks interaction.

### 8.2 Core Screens
1. **Workspace Home** — list of linked workspaces (active + archived), quick stats (docs indexed, last studied).
2. **Document View** — PDF Viewer / Lecture Viewer / Reference Book reader, with annotations and bookmarks, Assistant Panel docked right.
3. **Concept Graph View** — visual graph of extracted concepts and their relations, filterable by workspace.
4. **Memory & Analytics View** — weakness tracking, revision planner, mastery over time.
5. **Quiz / Exam Mode** — full-focus mode, Assistant Panel hidden or restricted, distinct from normal study flow.
6. **Research Mode** — multi-document, cross-workspace assistant context for literature-review-style work.
7. **Settings** — Ollama connection, model-per-engine overrides (advanced/optional), indexing preferences, storage locations.

### 8.3 What The UI Must Never Do
- Never open to a blank chat box as the landing screen.
- Never require the user to pick a model or a "mode" before asking a question.
- Never let the Assistant Panel become the majority of the screen by default.

## 9. Navigation Flow

```
App Launch
  → Last active Workspace (or Workspace Home if none)
      → Sidebar file tree selection
          → Document View (Assistant Panel available)
              → Ask a question → Assistant Panel shows answer + citations back into the document
              → Open Concept Graph from a highlighted concept
              → Enter Quiz/Exam Mode from a document or workspace
      → Global Search (hybrid search across active workspace or all workspaces)
      → Memory & Analytics (independent of any single document)
```

Navigation is always workspace-scoped by default; cross-workspace actions (global search, Research Mode) are explicit, opt-in states.

## 10. Folder Structure (Frozen)

```
/app
  /src                      # React + TypeScript frontend
    /components             # shared UI components (shadcn/ui-based)
    /views                  # screen-level components (§8.2)
    /panels                 # AssistantPanel, Sidebar, StatusBar
    /state                  # frontend state management (§13)
    /ipc                    # typed wrappers around Tauri IPC commands (§12)
    /styles                 # Tailwind config, theme tokens
  /src-tauri                # Tauri shell + Rust backend
    /src
      /commands              # IPC command handlers, thin, delegate to core
      /core                  # domain logic, engine-agnostic
        /workspace           # workspace lifecycle (§6.1)
        /watcher              # Folder Watcher (§21)
        /indexing              # OCR/parse/embedding pipelines (§17, §18)
        /engines                # Model Scheduler + Engines (§14)
        /memory                  # Student Memory logic (§19)
        /graph                    # Concept Graph logic (§20)
        /db                        # SQLite access layer (§10.1 below, §21)
        /vector                     # Vector DB access layer
      /main.rs
    Cargo.toml
  /docs
    README.md                # this file
/scripts                     # dev/build scripts only
```

Rules:
- No new top-level directory under `/app` or `/src-tauri/src` without amending this section.
- `core` modules MUST NOT import from `commands`; dependency direction is one-way: `commands → core`.
- Frontend `state` MUST NOT call `fetch`/HTTP directly; all backend communication goes through `ipc`.

> **Amendment note (see §46's own rules on explicit amendment):** the physical crate layout described in §10 above was superseded by an explicit architecture clarification during implementation. §11's crate-boundary model is authoritative for the physical file layout; §10's `/src-tauri/src/core/*` tree is retained here as the *conceptual* domain grouping it always represented, mapped onto §11's crates as follows: `commands` → `crates/app-tauri/src/commands`, `core/workspace` → `crates/atlas-workspace`, `core/watcher` → `crates/atlas-watcher`, `core/indexing` → `crates/atlas-indexer` (+ `atlas-vector` for the embedding/vector-DB side), `core/engines` → `crates/atlas-models`, `core/memory` → `crates/atlas-memory`, `core/graph` → `crates/atlas-graph`, `core/db` → `crates/atlas-db`. Four additional foundational crates not enumerated in the original §11 list were introduced as part of this same clarification: `atlas-types` (shared DTOs), `atlas-utils` (error/logging), `atlas-config` (Settings/§23 provider), and `atlas-events` (Event Bus/§34 interface). Crate naming uses the `atlas-*` prefix rather than `core-*`. This note documents the amendment; it does not reopen §10 for further change.

## 11. Rust Crate Layout

Single Cargo workspace, one primary binary crate (`src-tauri`) plus internal library crates for testability:

```
/src-tauri
  Cargo.toml               # workspace root
  /crates
    core-workspace          # workspace + folder link lifecycle
    core-watcher             # filesystem watching, debouncing, event → indexing job translation
    core-indexing             # OCR, parsing, chunking, embedding orchestration
    core-engines               # Model Scheduler + Engine trait implementations
    core-memory                 # Student Memory domain logic
    core-graph                   # Concept Graph domain logic
    core-db                       # SQLite schema + queries (sqlx or rusqlite)
    core-vector                    # Vector DB client abstraction
    app-tauri                       # the actual Tauri binary; wires commands to core crates
```

- Each `core-*` crate MUST be usable and testable without Tauri (no `tauri::` imports inside `core-*`).
- `app-tauri` is the only crate allowed to depend on `tauri`.
- Cross-crate contracts are plain Rust types (structs/enums), serializable via `serde`, shared through a `core-types` crate if duplication becomes a problem — adding `core-types` requires amending this section first.

> **Amendment (authoritative naming/dependency graph):** per explicit instruction during implementation, this section is amended as follows, superseding the crate names above (the `core-types` provision above is fulfilled by `atlas-types`):
>
> - Crate prefix is `atlas-*`, not `core-*`: `atlas-workspace`, `atlas-watcher`, `atlas-indexer` (renamed from `core-indexing`), `atlas-models` (renamed from `core-engines`), `atlas-memory`, `atlas-graph`, `atlas-db`, `atlas-vector`, `app-tauri`.
> - Four additional crates are added: `atlas-types` (shared DTOs, fulfilling the `core-types` provision above), `atlas-utils` (error type + logging bootstrap), `atlas-config` (Settings/§23 provider interface), `atlas-events` (Event Bus/§34 interface).
> - A new composition-root crate, `atlas-core`, is added. It is the single place the full dependency graph is visible; it wires concrete infrastructure (`atlas-db`, `atlas-vector`) behind the interfaces domain crates depend on, and exposes a facade (`AppFacade`) to `app-tauri`.
> - Dependency direction: `app-tauri → atlas-core → {atlas-workspace, atlas-watcher, atlas-indexer, atlas-models, atlas-memory, atlas-graph, atlas-events, atlas-config, atlas-types, atlas-utils, atlas-db, atlas-vector}`. Domain crates (`atlas-workspace`, `atlas-indexer`, `atlas-memory`, `atlas-graph`, `atlas-models`) define repository/provider traits; `atlas-db` and `atlas-vector` implement them (Dependency Inversion). Domain crates never depend on `atlas-db` or `atlas-vector` (forbidden edges: Workspace → SQLite, Indexer → SQLite, etc., preserved from §46).
> - `app-tauri` remains the only crate depending on `tauri`, and depends only on `atlas-core` (plus `atlas-utils`) — never directly on `atlas-db`, `atlas-vector`, or `atlas-models` — preserving "UI → Database" and "UI → Ollama" as forbidden edges.

## 12. IPC Design

All frontend↔backend communication goes through typed Tauri `invoke` commands. No ad-hoc HTTP servers, no WebSocket layer, unless a future amendment adds a specific streaming need.

Conventions:
- Command names are `snake_case`, grouped by domain: `workspace_link`, `workspace_list`, `workspace_archive`, `document_open`, `assistant_ask`, `memory_get_weaknesses`, `graph_get`, etc.
- Every command has a single typed request struct and a single typed response struct (or a `Result<T, AppError>`), mirrored on the TypeScript side via generated or hand-maintained types in `/src/ipc`.
- Long-running operations (indexing, assistant answers) use Tauri's event system to stream progress/tokens back to the frontend (`workspace://indexing-progress`, `assistant://answer-stream`), rather than blocking a single request/response call.
- Errors are structured (`AppError { code, message, context }`), never raw strings, so the UI can render consistent error states.

## 13. State Management

Frontend:
- Local UI state: React component state / hooks.
- Cross-cutting app state (active workspace, active document, assistant panel state, indexing status): a single lightweight global store (e.g. Zustand) under `/src/state`. No Redux, no additional state library, without amendment.
- Server-derived state (documents, memory, graph data) is fetched via `/src/ipc` and cached in the store; the store is a cache of backend truth, not a second source of truth.

Backend:
- SQLite is the source of truth for structured state (workspaces, documents, memory, settings).
- Vector DB is the source of truth for embeddings/semantic search only; it is always rebuildable from SQLite + source files (it lives in AI Cache territory, §7.2).
- In-memory backend state (current indexing jobs, scheduler queue) is transient and reconstructable on restart from SQLite job records.

## 14. Backend Modules

Top-level backend module boundaries (map to crates in §11):

- **Workspace Module** — link/unlink/archive workspaces, track root folders.
- **Watcher Module** — filesystem event capture, debounce, translate to indexing jobs.
- **Indexing Module** — OCR Pipeline (§17), parsing, chunking, Embedding Engine calls, writes to AI Cache.
- **Engines Module** — houses all Engines and the Model Scheduler (§14.1 below, §15 details).
- **Memory Module** — Student Memory read/write, weakness scoring, revision planning logic.
- **Graph Module** — Concept Graph construction and querying.
- **DB Module** — SQLite schema, migrations, query layer.
- **Vector Module** — vector DB client, collection management per workspace.

### 14.1 Engines (not model names)

The system is designed around **capabilities**, never specific model identifiers, so the underlying models can change without touching application logic.

| Engine | Responsibility |
|---|---|
| Vision Engine | Image/diagram understanding within documents |
| OCR Engine | Text extraction from scanned/image-based content |
| Embedding Engine | Generates vector embeddings for chunks/queries |
| Retriever | Hybrid (vector + keyword) retrieval over a workspace |
| Reranker | Reorders retrieved candidates by relevance |
| Tutor Engine | Explains, teaches, answers in a pedagogical style |
| Reasoning Engine | Multi-step reasoning, problem solving |
| Planner | Builds/updates revision plans, study schedules |
| Memory Engine | Reads/writes Student Memory, scores weaknesses |
| Analytics Engine | Aggregates memory/progress data for the Analytics view |
| Model Scheduler | Routes a request through the correct sequence of engines |

Rule: application code (UI, IPC handlers, business logic) MUST refer to engines by these names, never by underlying Ollama model name. Model-to-engine binding is configuration (§23), not code.

## 15. Model Scheduler

The Scheduler is the only component that decides which Engines run for a given request, and in what order. It MUST NOT call every engine for every request.

Example flow (illustrative, not the only pipeline shape):

```
User Question
   ↓
Intent Detection        (classify: factual lookup / tutoring / quiz / research / planning)
   ↓
Retrieval                (Retriever, using Embedding Engine + keyword index)
   ↓
Rerank                    (Reranker narrows to top candidates)
   ↓
Tutor Engine or           (depending on intent)
Reasoning Engine
   ↓
Verification               (optional pass, checks answer against retrieved sources)
   ↓
Answer                      (streamed back to Assistant Panel, with citations into source docs)
```

Other intents route differently: a quiz-generation request skips straight to Reasoning Engine + Memory Engine (to target weak concepts); a planning request routes to Memory Engine → Planner without touching Retrieval at all. The exact pipeline table per intent lives in `core-engines` as data (a routing table), not hardcoded per-feature branching, so new intents can be added by extending the table rather than rewriting control flow.

## 16. Knowledge Engine (Overview)

The Knowledge Engine is the umbrella term for everything that turns raw linked files into queryable knowledge: Indexing Module (§17, §18) + Vector Module + Graph Module (§20). It is not a separate crate; it's the conceptual pipeline that spans `core-indexing`, `core-vector`, and `core-graph`.

## 17. OCR Pipeline

```
File change detected (Watcher)
   → File type detection
   → If text-native (e.g. text PDF, markdown, docx): direct text extraction
   → If image-based (scanned PDF, slide images): OCR Engine pass
   → Normalize to a common Document/Page/Block structure
   → Persist parsed structure to AI Cache (SQLite: documents/blocks tables)
```

- OCR runs only on pages/files that need it (detected, not assumed).
- OCR output is versioned by source file hash; re-OCR only happens if the file content changes.

## 18. Search Pipeline (Hybrid Search)

```
Query
   → Keyword search (SQLite FTS5 or equivalent) over parsed text
   → Vector search (Embedding Engine → Vector DB) over chunk embeddings
   → Merge + Rerank (Reranker)
   → Results scoped to active workspace by default; cross-workspace only in Research Mode / Global Search
```

Chunking strategy, embedding model choice, and reranking model are configuration bound to the Embedding Engine / Reranker (§14.1), not hardcoded in the pipeline logic.

## 19. Student Memory

Tracks, per workspace and per concept:
- Quiz/exam attempt history and scores.
- Weakness scores per concept (derived from quiz performance + explicit user feedback).
- Revision schedule state (spaced-repetition-style scheduling, engine: Planner).
- Annotations and bookmarks tied to specific documents/locations.
- Workspace-specific chat/assistant interaction history.

Student Memory is append-heavy and durable; see §7.3 for deletion rules. It is the one dataset that MUST survive workspace archival and source file loss.

## 20. Concept Graph

- Concepts are extracted during indexing (Reasoning Engine / Tutor Engine assist in labeling; exact extraction pipeline is an indexing-time job, not a live query-time job).
- Graph nodes: Concepts. Edges: relationships (prerequisite-of, related-to, part-of), weighted by co-occurrence and explicit links.
- Stored relationally in SQLite (nodes/edges tables) — no dedicated graph database, to keep the storage story simple and consistent with §5.
- Concept Graph View (§8.2.3) queries this data read-only; graph construction/updates happen during indexing, not on every view render.

## 21. Background Workers

- **Folder Watcher**: OS-level filesystem watcher (per linked workspace root), debounces rapid changes, emits indexing jobs.
- **Indexing Worker Pool**: consumes indexing jobs (OCR, parse, embed), bounded concurrency, reports progress via IPC events (§12).
- **Scheduler Worker**: executes engine pipelines for in-flight assistant requests; not the same pool as indexing (assistant responsiveness must not wait behind a large re-index).
- All background workers persist their job queue state in SQLite so an app restart mid-index resumes rather than restarts from zero.

## 22. Caching Strategy

- AI Cache (§7.2) is a cache in the literal sense: fully regenerable from Source Documents. It MAY be cleared and rebuilt by the user from Settings.
- Cache invalidation key: source file content hash + parser/engine version tag. If either changes, the cached artifact is stale and is regenerated on next indexing pass, not eagerly.
- Vector DB collections are namespaced per workspace, so clearing/rebuilding one workspace's cache never touches another's.

## 23. Settings

- Ollama connection settings (host/port, defaults to local instance).
- Optional advanced per-engine model overrides (which local Ollama model backs the Embedding Engine, Tutor Engine, etc.) — optional because the default binding is sane out of the box; this is the only place model names appear in the UI.
- Indexing preferences (which file types to watch, OCR on/off per workspace, concurrency limits).
- Storage locations for AI Cache / Student Memory / Vector DB data (defaults to an app data directory, user-overridable).
- Data management: rebuild cache, archive/unarchive workspace, export Student Memory, destructive delete flows (explicit, confirmed, separate from normal unlink).

## 24. Error Handling

- All backend errors are structured (`AppError`, §12), carrying a stable `code` for programmatic handling and a human `message` for display.
- Categories: `FileSystemError`, `IndexingError`, `EngineError` (e.g. Ollama unreachable), `DbError`, `VectorDbError`, `ValidationError`.
- Engine/Ollama failures degrade gracefully: if the Tutor Engine's backing model is unavailable, the Assistant Panel shows a clear inline error and a link to Settings — it MUST NOT silently fail or produce a fabricated answer.
- Indexing errors on a specific file are recorded per-file (visible as a badge in the Sidebar) and do not halt indexing of the rest of the workspace.

## 25. Performance Goals

- App cold start to interactive Workspace Home: target under 2 seconds on typical hardware.
- Opening a previously indexed document: near-instant (<300ms to render, streaming for large PDFs).
- Incremental indexing of a single changed file: should not noticeably block UI interaction (background worker, §21).
- Assistant first-token latency: dominated by local model inference, not app overhead; app-side overhead (retrieval + rerank before generation) should target well under 1 second on typical hardware.
- Initial full-workspace indexing time is expected to scale with corpus size and is explicitly allowed to take longer, as long as the UI remains usable and progress is visible.

## 26. Coding Standards

- Rust: `rustfmt` + `clippy` clean, no `unwrap()` in `core-*` crates outside tests (use `Result` + `AppError`).
- TypeScript: strict mode on, no implicit `any`, ESLint + Prettier enforced.
- No business logic in Tauri command handlers (`commands/`) — handlers validate input, call into `core`, map errors, and return. Logic lives in `core-*`.
- No business logic in React components beyond presentation and local UI state — data fetching/mutation goes through `/src/ipc`, cross-cutting state through `/src/state` (§13).

## 27. Naming Conventions

- Engines are always referred to by role name (§14.1 table), never by underlying model name, anywhere in code, UI copy, or docs.
- Rust: crates and modules `snake_case`; types `PascalCase`; IPC command functions `snake_case` matching the command string used from the frontend.
- TypeScript: components `PascalCase`; hooks `useCamelCase`; IPC wrapper functions mirror backend command names in `camelCase` (e.g. backend `workspace_link` → frontend `workspaceLink`).
- Database tables: `snake_case`, plural (`documents`, `workspaces`, `memory_events`, `concept_nodes`, `concept_edges`).
- No component, module, crate, or table may be renamed without amending this section.

> **Amendment note:** crate naming was explicitly amended from `core-*` to `atlas-*` (see §11 amendment above). All other naming conventions in this section are unchanged.

## 28. Future Extension Rules

This contract is frozen, but the product will grow. Extensions MUST follow these rules:

1. New capabilities are added as new **Engines** (§14.1) behind the existing Scheduler routing table (§15), not as bespoke one-off code paths.
2. New Engines are added to the table in §14.1 before any implementation begins — this document is amended first, code follows.
3. New IPC commands follow the existing naming and error-handling conventions (§12, §24); no parallel communication mechanism is introduced without amendment.
4. New storage needs are additive to the existing three-category model (§7) — nothing new is invented that doesn't fit Source Documents / AI Cache / Student Memory.
5. No new top-level dependency (frontend or backend) is added without a written justification appended to §5.

## 29. Security

- No network exposure by default: the app does not open any listening port beyond what's required to talk to a local Ollama instance.
- No remote code execution surfaces: documents are parsed/rendered defensively (sandboxed PDF rendering, no execution of embedded scripts/macros from source documents).
- No credentials, API keys, or auth tokens exist in this system (no cloud services, §4) — there is nothing to leak by design.
- File access is strictly read-only for Source Documents; write access is confined to the app's own data directories (AI Cache, Student Memory, Settings, Vector DB).
- Local data at rest is not encrypted by default in v1 (no cloud sync to protect against); full-disk/user-level OS security is assumed. Encryption-at-rest may be considered as a future amendment, not a v1 requirement.

## 30. Testing Strategy

- `core-*` Rust crates: unit tests for domain logic (workspace lifecycle transitions, cache invalidation rules, scheduler routing table resolution), independent of Tauri.
- Integration tests: spin up a temp SQLite DB + temp filesystem workspace, exercise indexing pipeline end-to-end with fixture files (including at least one scanned/OCR-required fixture).
- Frontend: component tests for views/panels using fixture IPC responses (no real backend needed); a smaller set of end-to-end tests (Tauri's e2e tooling) for critical flows (link workspace → indexing → ask a question → get an answer).
- Scheduler routing table is data-driven (§15), so it gets dedicated table-driven tests: given an intent, assert the expected engine sequence.

## 31. Deployment

- Distributed as a native desktop installer per OS (Tauri's bundler: `.dmg`/`.app` for macOS, `.msi`/`.exe` for Windows, `.AppImage`/`.deb` for Linux).
- No auto-update-to-cloud requirement in v1; if auto-update is added later, it must fetch from a static release artifact source, not introduce a backend service (§4 non-goals).
- Ollama is a separate, user-installed dependency; the app detects it at runtime and guides the user to install/start it if missing, rather than bundling or managing it.
- No installation-time network requirement beyond what the OS package manager itself needs.

## 32. Development Rules

1. Do not redesign the architecture described in this document.
2. Do not change the folder structure (§10) or crate layout (§11).
3. Do not invent new top-level modules/engines outside the table in §14.1 without amending this document first.
4. Do not rename components, crates, tables, or IPC commands (§27) unless explicitly instructed.
5. Do not add new dependencies without a recorded justification (§5, §28.5).
6. Treat this README as frozen: implementation should conform to it, not the other way around.
7. When a future prompt seems to require deviating from this document, the correct response is to flag the conflict and ask whether to amend this contract — not to silently diverge.

---

# Addendum — Architecture Gap Fill (Sections 33–48)

> **Status:** FROZEN, same as Sections 1–32. This addendum does not modify, rename, reorganize, or remove anything in Sections 0–32. It fills architectural gaps that must exist for Sections 1–32 to be implementable without ad-hoc decisions during coding. Once written, these sections are equally frozen. Future implementation problems are resolved by proposing the smallest possible change here, not by redesigning.

## Governing Principle: Configuration Over Hardcoding, Interfaces Over Implementations

Two rules apply to every section below and to all implementation that follows from them:

1. **No hardcoded configuration.** Model names, model paths, workspace paths, directories, file extensions, chosen OCR/embedding/vector-DB implementations, chunk sizes, retrieval parameters, prompt templates, colors/themes/layout constants, feature flags, ports, endpoints, and any other value that may plausibly change MUST be read from configuration (§37 Model Registry, §23 Settings, or a dedicated config owner), never embedded in business logic.
2. **Dependency Inversion everywhere.** High-level modules (Engines, Workspace logic, Memory logic) depend only on interfaces/contracts (`*Repository`, `*Provider`, `*Client` traits in Rust terms — described here architecturally, not implemented). Concrete implementations (SQLite, Qdrant/LanceDB, a specific Ollama model, a specific OCR library) are plugged in beneath the interface and are swappable without touching the module that depends on them.

```
Workspace Engine
   ↓ depends on
Workspace Repository (interface)
   ↓ implemented by
SQLite Workspace Repository

Tutor Engine
   ↓ depends on
Model Provider (interface)
   ↓ resolved via
Model Registry (§37) → currently-selected model
```

This pattern applies identically to OCR, Vision, Embeddings, Retriever, Tutor, Memory, and Search — each has an interface owned by `core-*` domain logic, and a concrete adapter owned by an infrastructure module. This does not add new top-level crates beyond §11; adapters live inside their existing owning crate (e.g. the SQLite adapter for Workspace Repository lives in `core-db`, consumed by `core-workspace`).

---

## 33. Database Schema

Architecture only — no SQL. Each table's **ownership** names the crate (§11) responsible for writing to it; other crates MUST read/write only through that crate's repository interface, never touch the table directly.

### 33.1 `workspaces`
- **Purpose**: one row per linked workspace root (§6, §6.1).
- **Columns (conceptual)**: id, root_path, display_name, status (unlinked/linking/indexing/active/archived), created_at, last_indexed_at.
- **Relations**: parent of `documents`.
- **Indexes**: on `status`, on `root_path` (uniqueness).
- **Ownership**: `core-workspace`.

### 33.2 `documents`
- **Purpose**: one row per source file discovered under a workspace (§7.1).
- **Columns**: id, workspace_id, relative_path, content_hash, file_type, size, mtime, parse_status, last_indexed_hash.
- **Relations**: belongs to `workspaces`; parent of `chunks`, `annotations`, `bookmarks`.
- **Indexes**: on `workspace_id`, on `content_hash` (for orphan re-attachment, §7.3), on `parse_status`.
- **Ownership**: `core-indexing`.

### 33.3 `chunks`
- **Purpose**: normalized text/content units produced by the Parser Layer (§35) for retrieval (§18).
- **Columns**: id, document_id, sequence_index, text_content, page_or_location_ref, token_count, parser_version.
- **Relations**: belongs to `documents`; referenced by `embeddings_metadata`.
- **Indexes**: on `document_id`, on `(document_id, sequence_index)`.
- **Ownership**: `core-indexing`.

### 33.4 `embeddings_metadata`
- **Purpose**: relational pointer from a `chunk` to its vector in the Vector DB (the vector itself lives in Qdrant/LanceDB, not SQLite, per §5).
- **Columns**: id, chunk_id, vector_db_collection, vector_id, embedding_provider_id (→ Model Registry, §37), created_at.
- **Relations**: belongs to `chunks`.
- **Indexes**: on `chunk_id`, on `vector_db_collection`.
- **Ownership**: `core-vector`.

### 33.5 `concept_nodes`
- **Purpose**: extracted concepts (§20).
- **Columns**: id, workspace_id, label, description, created_at.
- **Relations**: parent of `concept_edges` (both directions); referenced by `learning_progress`.
- **Indexes**: on `workspace_id`, on `label`.
- **Ownership**: `core-graph`.

### 33.6 `concept_edges`
- **Purpose**: relationships between concepts (§20).
- **Columns**: id, from_node_id, to_node_id, relation_type (prerequisite-of / related-to / part-of), weight.
- **Relations**: both ends belong to `concept_nodes`.
- **Indexes**: on `from_node_id`, on `to_node_id`.
- **Ownership**: `core-graph`.

### 33.7 `student_memory`
- **Purpose**: umbrella conceptual grouping for the durable memory tables below (§7.3, §19). Implemented as several tables rather than one wide table.
- **Ownership**: `core-memory` owns all tables in this group.

### 33.8 `annotations`
- **Purpose**: user-authored annotations on a document (§8.2.2, §19).
- **Columns**: id, document_id, location_ref, content, created_at, updated_at.
- **Relations**: belongs to `documents`.
- **Indexes**: on `document_id`.
- **Ownership**: `core-memory`.

### 33.9 `bookmarks`
- **Purpose**: saved locations within a document (§19).
- **Columns**: id, document_id, location_ref, label, created_at.
- **Relations**: belongs to `documents`.
- **Indexes**: on `document_id`.
- **Ownership**: `core-memory`.

### 33.10 `chat_sessions`
- **Purpose**: workspace-specific assistant conversations (§19, "Workspace-specific chats" in §8's feature list).
- **Columns**: id, workspace_id, document_id (nullable — set when scoped to a single document), title, mode (normal/research/exam-restricted), created_at, updated_at.
- **Relations**: belongs to `workspaces`; optionally to `documents`; parent of `chat_messages`.
- **Indexes**: on `workspace_id`, on `document_id`.
- **Ownership**: `core-memory`.

### 33.11 `chat_messages`
- **Purpose**: individual turns within a chat session.
- **Columns**: id, session_id, role (user/assistant), content, engine_pipeline_used (references the §15 routing decision taken), created_at.
- **Relations**: belongs to `chat_sessions`.
- **Indexes**: on `session_id`, on `(session_id, created_at)`.
- **Ownership**: `core-memory`.

### 33.12 `settings`
- **Purpose**: single centralized configuration store (§23, and the Governing Principle above).
- **Columns**: key, value, value_type, scope (global/workspace), workspace_id (nullable), updated_at.
- **Relations**: optionally belongs to `workspaces` when scope=workspace.
- **Indexes**: unique on `(key, scope, workspace_id)`.
- **Ownership**: a dedicated settings module inside `core-db`, read by all crates through a `SettingsProvider` interface — no crate reads this table directly.

### 33.13 `model_registry`
- **Purpose**: backing store for the Model Registry (§37).
- **Columns**: id, model_identifier, engine_role (references §14.1 engine names), capabilities (json), context_length, vram_requirement, status (available/loading/unavailable/error), version, supported_tasks (json), is_selected_for_role (bool).
- **Relations**: none (flat registry, referenced conceptually by Engines via the Model Registry interface, never by foreign key from business tables).
- **Indexes**: on `engine_role`, on `status`.
- **Ownership**: `core-engines`.

### 33.14 `jobs`
- **Purpose**: backing store for the Background Job System (§36), enabling resume-after-restart (§21).
- **Columns**: id, job_type, payload (json), status (queued/running/succeeded/failed/cancelled), priority, retry_count, max_retries, created_at, started_at, completed_at, error (nullable).
- **Relations**: payload references entities (e.g. document_id) loosely, by id, not by hard foreign key, since job payloads are heterogeneous.
- **Indexes**: on `status`, on `(status, priority)`, on `job_type`.
- **Ownership**: `core-indexing` (job queue implementation shared via interface, described in §36).

### 33.15 `events`
- **Purpose**: durable log of application events (§34) for debugging, analytics, and any subscriber that needs replay (e.g. Analytics Engine catching up after downtime).
- **Columns**: id, event_type (e.g. WorkspaceAdded, IndexCompleted), payload (json), occurred_at.
- **Relations**: none (loosely references entities via payload, same rationale as `jobs`).
- **Indexes**: on `event_type`, on `occurred_at`.
- **Ownership**: the Event Bus implementation (§34), physically located in `core-db` as a generic append log, written to only through the Event Bus interface.

### 33.16 `analytics`
- **Purpose**: pre-aggregated data backing the Analytics Engine (§14.1) and Memory & Analytics View (§8.2.4), so the UI doesn't recompute aggregates on every render.
- **Columns**: id, workspace_id, metric_key, metric_value, computed_at, period (day/week/month/all-time).
- **Relations**: belongs to `workspaces`.
- **Indexes**: on `(workspace_id, metric_key, period)`.
- **Ownership**: `core-memory` (Analytics Engine writes here; this is a materialized/cache table, regenerable from `student_memory` group tables — it belongs conceptually to AI Cache, §7.2, even though it's derived from Student Memory data).

### 33.17 `revision_history`
- **Purpose**: log of the Planner's revision-schedule decisions and outcomes (§19, §21).
- **Columns**: id, concept_node_id, scheduled_at, completed_at (nullable), outcome (nullable — e.g. recalled/forgotten), created_at.
- **Relations**: belongs to `concept_nodes`.
- **Indexes**: on `concept_node_id`, on `scheduled_at`.
- **Ownership**: `core-memory`.

### 33.18 `learning_progress`
- **Purpose**: current mastery/weakness state per concept, the read model that `revision_history` and quiz attempts feed into (§19).
- **Columns**: id, concept_node_id, mastery_score, weakness_score, last_reviewed_at, attempt_count.
- **Relations**: belongs to `concept_nodes` (one row per concept, updated in place; `revision_history` is the append log this is derived from).
- **Indexes**: on `concept_node_id` (unique).
- **Ownership**: `core-memory`.

---

## 34. Event System

An application-wide, in-process event bus decouples modules so that, per §16/Governing Principle, engines and background systems never call each other directly when an event relationship is more appropriate.

### 34.1 Responsibilities
- Publish events when significant state transitions occur.
- Deliver events to registered subscribers, in-process (no external message broker — this stays local-first per §5).
- Persist events to the `events` table (§33.15) for replay/debugging.
- Never carry business logic itself — the bus routes and logs; subscribers act.

### 34.2 Canonical Events (non-exhaustive, extensible per §28)

| Event | Published by | Typical subscribers |
|---|---|---|
| `WorkspaceAdded` | Workspace Module | Watcher Module, Indexing Module |
| `WorkspaceRemoved` | Workspace Module | Watcher Module |
| `FileAdded` | Watcher Module | Indexing Module |
| `FileUpdated` | Watcher Module | Indexing Module |
| `FileDeleted` | Watcher Module | Indexing Module (cache invalidation, §22), Memory Module (orphan handling, §7.3) |
| `IndexCompleted` | Indexing Module | Graph Module, Vector Module, UI (via IPC event, §12) |
| `JobFailed` | Background Job System (§36) | UI (status bar, §8.1), Error Handling (§39) |
| `ModelLoaded` | Model Registry (§37) | Engines Module, UI (Settings, §23) |
| `ModelUnavailable` | Model Registry (§37) | Scheduler (§15), UI |
| `ChatStarted` | Memory Module | Analytics Engine |
| `ConceptUpdated` | Graph Module | Memory Module (learning_progress recompute), UI (Concept Graph View) |
| `MemoryUpdated` | Memory Module | Analytics Engine, Planner |

### 34.3 Publisher/Subscriber Rules
- A module MAY publish events about its own domain only (Workspace Module publishes `WorkspaceAdded`, never `FileAdded`).
- A module subscribing to an event MUST NOT assume delivery order relative to other subscribers of the same event.
- Subscribers MUST be idempotent where practical (safe to process the same event twice), since job/event replay after a crash (§36, §33.15) may redeliver.
- The Event Bus interface lives conceptually alongside `core-db` (owns the `events` table) but is exposed to every `core-*` crate as a shared, dependency-inverted interface — no crate constructs its own ad-hoc pub/sub.

---

## 35. Document Abstraction Layer

To satisfy §17 (OCR Pipeline) and §18 (Search Pipeline) without format-specific logic leaking into indexing, retrieval, or the UI, every supported source file is converted into one common internal representation before anything else touches it.

### 35.1 The Common Representation
Conceptually: a `Document` composed of ordered `Block`s, where each `Block` has a type (heading, paragraph, image, table, code, equation), a location reference (page number, character offset, slide index — whatever is meaningful for that source format), and normalized text content (empty for pure-image blocks pending OCR).

```
Document
  ├─ metadata (title, source path ref, file_type, content_hash)
  └─ Block[]
       ├─ type
       ├─ location_ref
       ├─ text_content
       └─ raw_ref (pointer back to original region, for viewer sync, §41)
```

### 35.2 How Formats Converge
- **PDF (digital)**: text layer extracted directly into Blocks; page number becomes `location_ref`.
- **PDF (scanned)**: image extracted per page, routed to OCR Engine (§17), OCR output becomes Block text_content, page number is still `location_ref`.
- **Markdown**: headings/paragraphs/code fences map directly to Block types; no OCR needed.
- **DOCX**: paragraph/heading/table structure maps to Block types via the DOCX Parser (§35.3 in §36... see §36.2 below).
- **PPTX**: each slide becomes a Block group; slide index is `location_ref`.
- **Images (standalone)**: single-Block document, routed through Vision Engine and/or OCR Engine.
- **Future formats**: any new format is required to produce the same `Document`/`Block` shape; this is the extension contract (§28) — a new Parser is added, the abstraction itself never changes.

### 35.3 Why This Matters
- Chunking (§33.3), the Concept Graph (§20), the Search Pipeline (§18), and the Viewer Contract (§41) all operate on `Document`/`Block`, never on format-specific structures. Adding a new file format touches only the Parser Layer (§36); it never touches indexing, retrieval, or UI code.

---

## 36. Parser Layer

Sits between raw files and the Document Abstraction Layer (§35). One Parser implementation per format, selected by a Parser Selector, all conforming to a single `Parser` interface (dependency inversion, per the Governing Principle).

### 36.1 Parser Selection
```
File detected (Watcher, §21)
   → File type detection (extension + content sniffing, not extension alone)
   → Parser Selector resolves: file_type → registered Parser implementation
   → Selected Parser produces a Document (§35.1)
```
The Parser Selector is a registry, not a hardcoded switch statement, so new parsers register themselves rather than requiring edits to a central if/else chain (this satisfies §28's extension rule and the no-hardcoding principle).

### 36.2 Parser Responsibilities by Format
- **Digital PDF Parser**: extract text layer + layout structure; detect and flag pages that are image-only (handing those off to OCR at the Block level rather than failing the whole document).
- **OCR PDF Parser**: rasterize page → OCR Engine (§17) → Block text_content; retains page image reference for viewer overlay (§41).
- **Markdown Parser**: structural parse (headings/lists/code/tables) → Blocks, no OCR.
- **Word (DOCX) Parser**: structural parse of paragraphs/headings/tables/images → Blocks; embedded images routed to Vision/OCR as needed.
- **PPTX Parser**: slide-by-slide parse, speaker notes captured as a distinct Block type (feeds Lecture Viewer, §8.2.2).
- **Image Parser**: wraps a raw image as a single-Block Document, defers content extraction to Vision Engine / OCR Engine.
- **Future Parsers**: MUST implement the same `Parser` interface (input: raw file handle; output: `Document`) and register with the Parser Selector; no other layer needs to change.

### 36.3 Boundary Rule
Parsers MUST NOT perform chunking, embedding, or retrieval logic — their sole responsibility ends at producing a valid `Document`. Chunking is a downstream Indexing Module step (§14, §33.3), keeping parsing, indexing, retrieval, and tutoring cleanly separated per §16/Governing Principle.

---

## 37. Model Registry

Backs §14.1's principle that application code never references model names directly. Table definition: §33.13.

### 37.1 Responsibilities
- Maintain the set of models discoverable from the local Ollama instance (and any other locally configured provider, should one be added in a future amendment).
- Store, per model: capabilities (e.g. text-generation, vision, embedding), context length, VRAM requirement, current status (available/loading/unavailable/error), version, supported tasks, and which Engine role(s) (§14.1) it is currently assigned to.
- Expose a `ModelProvider` interface to Engines: an Engine asks the Registry "give me the current model for `TutorEngine`," never "load `some-model-name`."
- React to `ModelLoaded` / `ModelUnavailable` events (§34.2) to keep status current.

### 37.2 Assignment, Not Hardcoding
Engine-to-model assignment is data in `model_registry` (§33.13), editable from Settings (§23), not a compiled-in mapping. Default assignments ship as default configuration values, not code constants — this is what allows swapping the underlying model for any Engine without touching Engine logic.

### 37.3 Ownership
`core-engines`, consumed by every Engine through the `ModelProvider` interface. No crate outside `core-engines` queries `model_registry` directly.

---

## 38. Resource Manager

Owns finite local hardware resources so that Engines never negotiate resource access with each other directly (satisfies the no-direct-engine-to-engine-communication rule in §16).

### 38.1 Responsibilities
- Track available CPU, RAM, GPU, and VRAM (where detectable) without assuming any specific OS or GPU vendor (no hardcoded OS/GPU assumptions, per the Governing Principle).
- Grant/deny/queue concurrent inference requests from the Scheduler (§15) based on current resource pressure, so a heavy indexing pass and an interactive assistant question don't starve each other.
- Allocate worker slots to the Indexing Worker Pool and Scheduler Worker (§21) from a shared, configurable concurrency budget (configured, not hardcoded).
- Expose current resource state to the Status Bar (§8.1) via IPC events (§12), and to Settings (§23) for user-adjustable concurrency limits.

### 38.2 Interaction Pattern
```
Scheduler wants to run Tutor Engine inference
   → asks Resource Manager for a slot
   → Resource Manager grants immediately, queues, or denies (with reason)
   → Scheduler proceeds or reports a structured error (§39)
```
The Resource Manager does not know what "Tutor Engine" means semantically — it only manages slots/budgets, keeping resource management and engine/business logic separated per the Governing Principle.

### 38.3 Ownership
`core-engines`, since resource negotiation is tightly coupled to the Scheduler (§15); exposed as a distinct internal interface so it can be tested and reasoned about independently.

---

## 39. Context Builder

A dedicated step between Retrieval (§18) and the Tutor/Reasoning Engines (§15), so retrieval, ranking, and prompt assembly stay separated per §16's "never mix parsing, indexing, retrieval and tutoring responsibilities."

### 39.1 Responsibilities
- **Hybrid retrieval intake**: accepts Retriever + Reranker output (§14.1, §18).
- **Ranking**: applies final ordering rules on top of Reranker output where context-assembly-specific ordering differs from pure relevance ranking (e.g. prioritizing the currently open document).
- **Compression**: trims verbose chunks to fit budget while preserving meaning (configurable strategy, not hardcoded truncation).
- **Deduplication**: removes near-duplicate chunks (e.g. overlapping page regions) before they consume token budget.
- **Token budgeting**: computes how much of the target model's context window (from Model Registry, §37) is available for retrieved context vs. conversation history vs. system/prompt overhead, and enforces that budget.
- **Ordering**: arranges surviving chunks in the sequence most useful to the downstream Engine (e.g. most relevant last, closest to the question, if that's the configured strategy).
- **Citation preparation**: attaches location_ref (§35.1) metadata to each surviving chunk so the eventual answer can cite back into the source document (needed for Viewer Contract sync, §41).
- **Context validation**: rejects/flags an assembled context that is empty, over-budget, or missing required citation metadata, before it ever reaches the Prompt Builder (§40).

### 39.2 Position in the Pipeline
```
Retriever → Reranker → Context Builder → Prompt Builder (§40) → Tutor Engine / Reasoning Engine
```
This refines (not replaces) the flow in §15 by making explicit the previously-implicit step between "Verification" and "Answer" — no existing §15 step is renamed or removed; the Context Builder is the detailed internal composition of preparing what §15 already calls the input to Tutor/Reasoning.

### 39.3 Ownership
`core-engines`, as a component used by the Scheduler ahead of invoking Tutor/Reasoning Engines.

---

## 40. Prompt Builder

Prompt construction MUST NOT occur inside any Engine. Every Engine that requires a prompt (Tutor, Reasoning, Planner, etc.) receives a fully assembled prompt string/object from the Prompt Builder — Engines never format their own prompts.

### 40.1 Responsibilities
- Assemble the final prompt from: system prompt (configurable template, §37/§23), workspace-level prompt additions (e.g. course-specific framing), user-level prompt preferences, the retrieved/assembled context (from Context Builder, §39), conversation history (from `chat_messages`, §33.11), and any tool outputs relevant to the current pipeline step.
- Resolve which template applies via configuration (prompt templates are data, stored/versioned as configuration, never string-literal constants inside Engine code — per the Governing Principle).
- Produce a single, fully-resolved prompt object handed to the target Engine; the Engine's only job is to run inference against it.

### 40.2 Position in the Pipeline
```
Context Builder (§39) → Prompt Builder → Engine (Tutor / Reasoning / Planner / etc.)
```

### 40.3 Ownership
`core-engines`, invoked by the Scheduler immediately before any Engine call that requires a prompt. Prompt templates themselves are configuration data owned alongside Settings (§23), not compiled into `core-engines`.

---

## 41. Startup Sequence

```
1. Load Configuration        (Settings table, §33.12, + defaults; establishes all non-hardcoded values before anything else runs)
2. Initialize Logging
3. Open Database              (SQLite connection, run pending migrations)
4. Load Workspaces             (read `workspaces` table, restore in-memory registry — does not start watchers yet)
5. Model Discovery               (query Ollama, populate/refresh Model Registry, §37; publish ModelLoaded/ModelUnavailable events, §34)
6. Start Watchers                  (Folder Watcher per active workspace, §21)
7. Start Background Workers          (Indexing Worker Pool, Scheduler Worker, resume any `jobs` rows left in-flight from a prior session, §36)
8. Initialize IPC                     (register all command handlers, §12/§42)
9. Launch UI                           (Tauri window; UI requests initial state via IPC once mounted)
10. Ready State                         (app signals readiness; Status Bar reflects steady state, §8.1)
```
Each step MUST complete (or explicitly, gracefully degrade — e.g. proceed with `ModelUnavailable` rather than blocking startup) before the next begins, so failures are attributable to a specific stage.

---

## 42. Shutdown Sequence

```
1. Signal Shutdown Intent      (from UI close event or OS signal)
2. Stop accepting new Jobs      (Background Job System stops dequeuing new work, §36)
3. Drain/Cancel in-flight Jobs    (in-flight jobs either finish quickly or are marked cancellable-and-resumable, state persisted to `jobs`, §33.14)
4. Stop Watchers                   (Folder Watcher instances unregister cleanly)
5. Flush Caches                      (any in-memory-only derived state that should survive restart is flushed to AI Cache / SQLite)
6. Close Database                      (final checkpoint/flush, close connection)
7. Flush Logs
8. Release Resources                    (Resource Manager releases any held GPU/CPU handles, §38)
```
No step may be skipped to speed up shutdown; a job left un-persisted at shutdown is the specific failure mode this sequence exists to prevent (consistent with §21's "resume rather than restart" guarantee).

---

## 43. IPC Contract

Refines §12 with an explicit namespacing convention. No ad-hoc, unnamespaced commands are permitted.

### 43.1 Namespace Convention
Commands are named `domain.action`, grouped by the same domains used elsewhere in this document:

| Namespace | Examples |
|---|---|
| `workspace.*` | `workspace.link`, `workspace.list`, `workspace.archive` |
| `assistant.*` | `assistant.ask`, `assistant.cancel` |
| `rag.*` | `rag.search`, `rag.getContext` |
| `ocr.*` | `ocr.status`, `ocr.reprocess` |
| `memory.*` | `memory.getWeaknesses`, `memory.recordAttempt` |
| `graph.*` | `graph.get`, `graph.getConceptDetail` |
| `settings.*` | `settings.get`, `settings.set` |
| `jobs.*` | `jobs.list`, `jobs.cancel`, `jobs.retry` |

### 43.2 Rules
- Every command lives under exactly one namespace; a command that seems to span two domains is a sign the responsibility is unclear and must be resolved (assigned to the owning domain) rather than duplicated.
- New commands extend an existing namespace or, if a genuinely new domain is introduced, that addition amends this section explicitly (§28.3).
- Namespacing here is additive clarification of §12's existing `snake_case` command-naming rule (e.g. `workspace.link` corresponds to the backend handler `workspace_link` described in §12); §12 is not being changed, only made more specific.

---

## 44. Viewer Contract

Defines how the PDF/Lecture/Reference viewers (§8.2.2) stay synchronized with Selection, Highlights, Annotations, Current Page, Search, and the Assistant Panel — all built on the Document Abstraction Layer's `location_ref` (§35.1).

### 44.1 Core Concept: Shared Location Reference
Every viewer-addressable point (a selection, a highlight, an annotation, a search hit, an assistant citation) is expressed as a `location_ref` compatible with the Block structure defined in §35.1. This is what lets the Assistant Panel say "see page 4, paragraph 2" and have the Viewer jump there, regardless of source format.

### 44.2 Interactions
- **Selection → Assistant**: user selects text in the Viewer; the Assistant Panel can use that selection as ad-hoc context for the next question (via Context Builder, §39, treating it as a pinned high-priority chunk).
- **Assistant → Viewer (AI Overlay)**: an assistant answer's citations (prepared by Context Builder, §39.1) render as clickable references; clicking scrolls/jumps the Viewer to the corresponding `location_ref` and highlights it temporarily.
- **Annotations/Bookmarks**: created from the Viewer, persisted via `core-memory` (§33.8, §33.9), re-rendered as overlays on subsequent opens of the same document.
- **Search → Viewer**: a Hybrid Search hit (§18) is a `location_ref`; selecting a result opens the Viewer at that location.
- **Current Page tracking**: the Viewer emits its current page/location as local UI state (§13); this is used to scope "ask about this page" style assistant interactions but is not persisted unless the user bookmarks it.

### 44.3 Synchronization Rule
The Viewer never talks to Engines directly. All of the above flows through: Viewer (UI) → IPC (`rag.*`/`assistant.*`, §43) → Scheduler/Context Builder → back through IPC events → Viewer. This preserves the "UI never calls Ollama directly" rule (§16) and keeps the Viewer a presentation component, not a business-logic component (§26).

---

## 45. Error Handling Philosophy

Extends §24's structured `AppError` model with explicit categorization, so every failure has a defined handling path and none are silently ignored.

### 45.1 Categories
- **Recoverable**: the system can continue in a degraded mode (e.g. one file fails OCR; indexing continues for the rest of the workspace, per §17/§21).
- **Fatal**: the current operation cannot continue and must stop cleanly (e.g. database corruption detected at startup, §41 step 3).
- **Retryable**: transient by nature; the Background Job System (§36) applies its retry policy (bounded retry_count, per `jobs.max_retries`, §33.14).
- **User Errors**: caused by user input/action (e.g. linking a folder with no readable files); surfaced directly and actionably in the UI, not logged as a system fault.
- **System Errors**: internal faults (unexpected panics caught at boundaries, IO failures) — logged, surfaced as a generic-but-honest error, never fabricated as a normal result.
- **Model Errors**: Ollama unreachable, model load failure, inference timeout — routed through Model Registry status (§37) and surfaced via `ModelUnavailable` (§34.2), so the Assistant Panel can explain rather than silently degrade.
- **Workspace Errors**: root folder missing/unreadable (e.g. removable drive disconnected) — workspace enters a distinct "unavailable" sub-state without being auto-archived or auto-deleted, per §7/§6.1's preservation guarantees.

### 45.2 Non-Negotiable Rule
No failure is ever silently swallowed. Every caught error either (a) is handled and the system continues in a defined degraded state, with the failure recorded (`events`, §33.15, and/or `jobs.error`, §33.14), or (b) is surfaced to the user through a structured, honest UI error state (§24). A bare `catch` that discards an error without one of these two outcomes is a defect, not an acceptable shortcut.

---

## 46. Development Constraints — NON-NEGOTIABLE RULES

These apply on top of, and do not replace, §26 (Coding Standards) and §32 (Development Rules).

1. Never hardcode configuration — model names, paths, ports, chunk sizes, prompt templates, UI constants, or any value listed in the Governing Principle above. Everything configurable lives in Settings (§23) or the Model Registry (§37), with one clearly defined owning module.
2. Never duplicate responsibilities — each table (§33), each Engine (§14.1), each module (§14) has exactly one owner; no second module writes to another's table or reimplements another's role.
3. Never bypass engine interfaces — Engines are only invoked through the Scheduler (§15) using Model Registry-resolved models (§37); nothing calls a model directly.
4. Never let UI call Ollama directly — all inference is reached through IPC → Scheduler → Engine → Model Registry (§12, §43, §44.3).
5. Never let business logic access UI — `core-*` crates have no knowledge of React/Tauri UI types (§11).
6. Never let engines communicate directly when an event exists — cross-cutting notifications go through the Event Bus (§34), not direct calls between Engines.
7. Never introduce circular dependencies — the crate dependency direction defined in §10/§11 (`commands → core`, `core-* ` modules depending only downward on interfaces) is one-way.
8. Never mix parsing, indexing, retrieval, and tutoring responsibilities — enforced structurally by the Parser Layer (§36), Indexing Module (§14), Retriever/Context Builder (§18, §39), and Tutor/Reasoning Engines (§14.1) remaining distinct components.
9. Every module owns exactly one responsibility — if a change seems to require touching two modules for one conceptual reason, that's a signal the responsibility boundary needs review (an amendment), not a reason to blur the boundary silently.
10. Every public interface must remain backward compatible — once an IPC command (§43), repository interface, or Engine interface ships, its contract is additive-only (new optional fields/commands) unless an explicit amendment states otherwise.
11. Every architectural change requires explicit approval — nothing in Sections 0–46 is altered as a side effect of implementation work.
12. If uncertain, preserve the existing architecture instead of inventing a new one — when an implementation problem arises, report the issue and propose the smallest possible change; architecture changes are the last resort, not the default response.

---

*End of addendum. Sections 33–46 are, from this point forward, equally frozen alongside Sections 0–32. Any change to Sections 0–46 is an amendment and must be made explicitly and deliberately, not as a side effect of an unrelated implementation task.*

---

# Amendment Log

1. **Crate boundary authority (§10/§11):** §11 (independently compilable Cargo crates) is authoritative for physical layout; §10's `/src-tauri/src/core/*` tree is conceptual domain grouping only. See notes inline in §10 and §11.
2. **Crate naming (§11, §27):** `atlas-*` prefix instead of `core-*`.
3. **New foundational crates (§11):** `atlas-types`, `atlas-utils`, `atlas-config`, `atlas-events`, and the composition-root `atlas-core`, added per explicit instruction. Not present in the original §11 list.
4. **Skeleton-milestone scope:** this codebase currently implements project scaffolding only — interfaces, module structure, DI wiring, and IPC command shapes — with no business logic (OCR, embeddings, retrieval, tutoring, model inference). Method bodies in the persistence adapters (`atlas-db`) and the vector adapter (`atlas-vector`) are placeholders (`unimplemented!()`), to be filled in a dedicated future milestone.

---

# Known Environment Limitations

This section documents a **build-environment constraint of the current sandboxed development container**, not an architectural decision. The architecture (§0–§46 above, as amended) is unchanged and is expected to build normally on the intended development environment.

## Summary

Of the 14 Rust crates in the `atlas-*` workspace, **13 compile cleanly with zero warnings** in this container: `atlas-types`, `atlas-utils`, `atlas-config`, `atlas-events`, `atlas-workspace`, `atlas-watcher`, `atlas-indexer`, `atlas-vector`, `atlas-models`, `atlas-memory`, `atlas-graph`, `atlas-db`, and `atlas-core`.

The 14th crate, **`app-tauri`**, is source-complete and structurally correct (IPC command handlers per §43.1, `main.rs` startup wiring, `build.rs`, `tauri.conf.json`) but its **native link step against the real `tauri` crate cannot complete inside this container**.

## Root cause

1. **Toolchain age.** This container's only available Rust toolchain is `rustc`/`cargo` **1.75.0** (installed via `apt`, from Ubuntu 24.04's `noble` repositories). No newer toolchain is reachable from this container — `rustup`'s toolchain distribution host (`static.rust-lang.org`) is not on this container's network allowlist, and Ubuntu's package repositories do not carry a newer `rustc`. Numerous crates in the modern Tauri 2.x dependency tree (`time`, `idna_adapter`/`icu_*`, `indexmap`, `getrandom`, `uuid`, `serde_with`, `unicode-segmentation`, etc.) now require Rust's `edition2024` (stabilized in `rustc 1.85`) or an MSRV above 1.75, and fail to even parse with this toolchain.
2. **System library mismatch.** Ubuntu 24.04 ships only `webkit2gtk-4.1`. An older Tauri major version's `wry`/`javascriptcore-rs-sys` bindings hard-require the discontinued `webkit2gtk-4.0` / `javascriptcoregtk-4.0` pkg-config packages, which are not installable here. Tauri v2 -- the version actually pinned in `app-tauri/Cargo.toml` -- targets `webkit2gtk-4.1` and does not have this problem on a normal Ubuntu 24.04 machine with a current Rust toolchain.

## What was, and was not, done about it

- `app-tauri/Cargo.toml` depends on **`tauri = "2"`** / **`tauri-build = "2"`** — the actual intended target stack per §5, unmodified.
- No architecture was changed, no crate was renamed, no module was redesigned, no compatibility shim was introduced, no pkg-config file was faked, and no ABI shim was created to force a link in this container. An earlier exploratory attempt in this session temporarily pinned `app-tauri` to an older Tauri major version with a long chain of transitive-dependency version pins purely to test how far the container could get; that attempt was **fully reverted** once it became clear the underlying library mismatch was unresolvable without a shim. The current `app-tauri/Cargo.toml` reflects only the intended target versions.
- The native link step for `app-tauri` is simply **not exercised** in this container. This is disclosed, not hidden: running `cargo build --workspace` here will succeed for all 13 other crates and fail specifically at `app-tauri`'s dependency resolution/build step, with the errors described above.

## Expected outcome on the intended environment

Per the project's intended stack (Windows 11 + Visual Studio Build Tools + latest stable Rust + latest Tauri-compatible toolchain), `app-tauri` is expected to build and link normally:
- A current stable Rust toolchain (installed via `rustup`, e.g. 1.85+) satisfies every crate's MSRV/edition requirement without any version pinning.
- Visual Studio Build Tools provides the MSVC linker Tauri needs on Windows, and Tauri v2 on Windows uses the OS-provided WebView2 runtime rather than `webkit2gtk`, so the Linux-specific `webkit2gtk` version mismatch described above does not apply at all on that target.

No action is required against this document to build on the intended environment; this section exists solely so that a `cargo build --workspace` failure inside *this specific sandboxed container* is not mistaken for a defect in the architecture or the implementation.
