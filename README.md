# Atlas

Local Learning OS — a local-first, offline, single-user AI Learning
Operating System for desktop.

**The architecture contract lives at [`app/docs/README.md`](app/docs/README.md).**
It is the single source of truth for this project (frozen, per its own §0).
Read it before touching anything in `app/`.

## Current milestone

This repository currently contains the **project skeleton only**: Cargo
workspace, crate boundaries, IPC command shapes, frontend scaffold, and
tooling config. No business logic (OCR, embeddings, retrieval, tutoring,
model inference) has been implemented yet. See the architecture doc's
"Amendment Log" and "Known Environment Limitations" sections for the current
state and a disclosed build-environment constraint of this repo's original
sandboxed development container.

## Layout

```
/app            -- frontend (React/TS/Tailwind) + src-tauri (Rust workspace)
/scripts         -- dev/build/check scripts
/.github/workflows -- CI
```

## Getting started

```
cd app
npm install
npm run tauri -- dev      # or: ../scripts/dev.sh
```

See `scripts/check.sh` for the full lint/format/typecheck/clippy/test suite
also run in CI (`.github/workflows/ci.yml`).
