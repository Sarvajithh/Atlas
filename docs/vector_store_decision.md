# Vector Store Decision (V1.0 Part 4)

**Status:** Decision recorded. No migration performed. Architecture doc amended
formally (see `app/docs/README.md`, Amendment Log entry #5) rather than
silently — per this phase's explicit instruction not to change architecture
docs without a recorded amendment.

## Background

`app/docs/README.md` §5 mandates "Qdrant or LanceDB (embedded/local mode)"
for vector storage. The shipped implementation (`atlas-vector::EmbeddedVectorStore`)
is a custom, dependency-free, brute-force cosine-similarity index instead.
This has been an open, disclosed deviation since it was introduced — flagged
in `README.md`'s "Known Limitations" and in `atlas-vector/src/store.rs`'s own
doc comment — but never formally evaluated or amended. This document is that
evaluation.

## Option A: Keep `EmbeddedVectorStore`

**What it is:** one `Vec<VectorRecord>` per namespaced (per-workspace)
collection, held behind an `RwLock`, optionally flushed to
`<collection>.json` on every write. Search is O(n) brute-force cosine
similarity, sorted and truncated to the requested limit.

**Pros**
- Zero new dependencies. The stated reason it was introduced was that
  Qdrant's and LanceDB's Rust crates don't build against this project's
  disclosed toolchain constraint (`rustc`/`cargo` 1.75.0 in the sandboxed
  dev container — see `app/docs/README.md`'s "Known Environment
  Limitations"). That constraint is specific to the *development sandbox*,
  not necessarily the shipping target, but it has already caused real
  friction once.
- Already sits behind the same `VectorStore`/`VectorSearchRepository`
  interfaces the rest of the app depends on (Dependency Inversion, per the
  architecture doc's own Governing Principle) — swapping the
  implementation later touches only this one file and its wiring in
  `atlas-core::facade`, not any call site.
- Simple enough to fully understand and audit by reading ~260 lines. No
  embedded database engine, no separate index files with their own format
  versioning, no additional attack surface.
- Already has real persistence-to-disk and reload-on-restart, and 6 passing
  unit tests covering upsert/replace/delete/search/namespacing/reload.
- Matches §2.1 "single-user, local-first" and §13 "Vector DB is always
  rebuildable from SQLite + source files" — there is no requirement here
  that implies needing a production-grade ANN index.

**Cons**
- O(n) brute-force search. For a single student's single workspace corpus
  (realistically low thousands of chunks, not millions), this is fast in
  practice, but it does not scale the same way a real ANN index
  (HNSW/IVF) would to a very large corpus.
- No metadata filtering, no quantization, no incremental index structures
  — anything beyond flat cosine similarity would need to be built by hand.
- Persistence is "rewrite the whole collection's JSON file on every
  upsert/delete", which is O(n) per write, not appropriate at large scale
  (though fine at the scale this app targets).
- Deviates from the frozen architecture contract, which — even though
  disclosed — is exactly the kind of drift §46's "no change to Sections
  0–46 without an explicit amendment" rule exists to prevent from
  happening silently.

## Option B: Migrate to Qdrant or LanceDB (embedded/local mode)

**Pros**
- Matches the original architecture contract exactly.
- Real ANN search (ok not to worry about brute-force scaling as corpus
  size or number of workspaces grows).
- LanceDB in particular is designed for embedded, single-process,
  local-first use (no server process to manage), which fits §2.1 well;
  Qdrant's embedded mode is similar but historically has leaned more
  toward the server-oriented usage pattern.

**Cons**
- Confirmed build blocker in this project's disclosed dev sandbox
  (`app/docs/README.md`'s "Known Environment Limitations" section): the
  same MSRV/edition2024 toolchain-age issue that currently blocks
  `app-tauri`'s native link step also affects the modern dependency trees
  either crate would pull in. This is stated to be a sandbox-only issue,
  not expected on the intended Windows + current-Rust target — but it has
  not actually been verified end-to-end on that target with these crates
  added, and this phase has no way to verify it (no working `cargo` in
  this environment either — see final report).
- New dependency surface: both are non-trivial embedded databases with
  their own file formats, versioning, and failure modes to reason about,
  test, and keep working across app upgrades — meaningfully higher
  ongoing maintenance and development risk than the current ~260-line
  implementation.
- No current evidence of a corpus-size problem the current store actually
  has. This is a single-user, offline-first study tool; a student's
  indexed workspace is realistically in the thousands, not millions, of
  chunks. Brute-force cosine similarity over a few thousand vectors is
  sub-millisecond in practice, well inside the §25 performance goals
  ("app-side retrieval + rerank overhead ... well under 1 second").
- Migration itself is real engineering work with real risk: schema for
  collections, on-disk format for existing embedded vectors, a data
  migration path for anyone who already has an `EmbeddedVectorStore`
  collection on disk, and re-verifying every place `VectorStore` is
  exercised (RAG retrieval, research queries, concept graph extraction
  inputs) against the new backend.

## Recommendation

**Keep `EmbeddedVectorStore` (Option A) for V1.0.**

Given the offline-first, single-student-workspace scale this app targets,
the current store's O(n) brute-force search is not a real bottleneck today,
and swapping it for Qdrant/LanceDB right now would trade a small, fully
understood, already-tested implementation for a real dependency (with a
live, if sandbox-scoped, build-compatibility question mark) to solve a
performance problem that hasn't actually manifested. The interface boundary
(`VectorStore`/`VectorSearchRepository`) already makes this swappable later
without touching any call site, which is the whole point of having that
boundary — this is a "defer, not abandon" recommendation, not a permanent
architectural stance.

**Recommended trigger for revisiting this**: if real-world workspace corpus
sizes are observed to regularly exceed roughly 50,000–100,000 indexed
chunks per workspace (the point where O(n) brute-force scans start to be
individually perceptible, i.e. tens of milliseconds, rather than
imperceptible), or if multi-workspace cross-search becomes a first-class
feature that needs to scan many collections at once, re-run this evaluation
with real numbers from real usage rather than an estimate.

This decision has been recorded as Amendment Log entry #5 in
`app/docs/README.md` rather than silently changing §5's stated mandate —
the mandate is still on record as the original contract; this document and
that amendment entry record that the deviation is now a *reviewed and
accepted* one, not just a disclosed one.
