#!/usr/bin/env bash
# Dev script only (§10: /scripts is dev/build scripts only, no business logic).
# Runs the Tauri dev loop, which in turn starts the Vite dev server per
# vite.config.ts and tauri.conf.json's devUrl/frontendDist wiring.
set -euo pipefail
cd "$(dirname "$0")/../app"
npm run tauri -- dev
