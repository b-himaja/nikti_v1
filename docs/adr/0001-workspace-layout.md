# ADR 0001: Workspace layout

## Status
Accepted

## Context
Nikti's architecture fixes a strict layering — types → repositories →
engines → commands → UI (CLAUDE.md standing priority order #2) — and a
Rust core with a Tauri shell, a Python sidecar for Analysis gap-filling
only, and a React frontend (CLAUDE.md Part 3, frozen technology
decisions). The codebase needs a directory structure that makes that
layering visible and hard to violate by accident, rather than one derived
from convenience or a generic template.

## Decision
A Cargo workspace with two members:
- `crates/nikti-core` — all domain logic: `types/`, `repositories/`,
  `engines/`, `commands/`, `sidecar/` (Rust-side sidecar supervisor and
  `ComputeBackend`/`EngineContext`), each a submodule matching one layer
  of INV-010's boundary. Nothing in this crate depends on Tauri.
- `crates/nikti-app` — the Tauri v2 shell. Contains only thin
  `#[tauri::command]` wrappers (`ipc_commands.rs`) that forward to
  `nikti-core::commands` and map errors to strings for the IPC boundary
  (INV-022: components invoke commands, never engines/repositories
  directly).

Alongside the workspace:
- `sidecar/` — the Python package (`nikti_sidecar`), kept as a sibling of
  `crates/`, not nested inside either crate, since it's a separate
  runtime/process, not a Rust dependency.
- `src/` — the React frontend, mirroring the same layering on the UI
  side: `workspaces/` (INV-008: workspaces author/execute),
  `components/` (INV-022: never call engines/repositories directly),
  `store/` (Zustand, UI-only state per the frozen technology decision),
  `utils/ipc.ts` (the one place `invoke()` is called from).
- `docs/adr/` — this decision record set.
- `tests/invariants.rs` — the CLAUDE.md Part 4 invariant tests, kept at
  the repo root (not inside either crate) so they read as binding on the
  whole system, not on one crate's internals.

`nikti-core` also carries its own `tests/` (e.g.
`sidecar_supervisor.rs`) for tests that exercise its own internals
directly (spawning the real sidecar process) — those are ordinary
crate-level integration tests, distinct from the repo-root invariant
suite.

## Consequences
- A dependency from `nikti-core` on `tauri` is a structural violation
  visible immediately in `Cargo.toml`, not just a code-review nit.
- The Python sidecar being a sibling directory (not `crates/nikti-app/sidecar`
  or similar) keeps it visibly a separate process lifecycle, matching
  INV-024 (local child process) and the frozen sidecar contract.
- Frontend layering mirrors the Rust side closely enough that INV-022/023
  violations (a component calling something other than `utils/ipc.ts`)
  are easy to spot in a diff.
