#!/usr/bin/env bash
# Launches the full Nikti app in dev mode: starts the Vite frontend dev
# server, builds nikti-app, and opens the Tauri window pointed at it.
# Equivalent to `tauri dev`, wrapped because this repo's tauri.conf.json
# lives under crates/nikti-app rather than the conventional src-tauri/
# layout, and the Tauri CLI resolves beforeDevCommand relative to that
# directory's parent (crates/), not the workspace root.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Non-interactive shells here don't auto-load nvm (see CLAUDE.md-adjacent
# session notes) — source it explicitly so `npm`/node resolve correctly.
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
if [ -s "$NVM_DIR/nvm.sh" ]; then
  # shellcheck source=/dev/null
  . "$NVM_DIR/nvm.sh"
fi

cd "$ROOT_DIR/crates/nikti-app"
exec "$ROOT_DIR/src/node_modules/.bin/tauri" dev
