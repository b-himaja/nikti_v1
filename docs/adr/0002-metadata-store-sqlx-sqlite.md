# ADR 0002: Metadata store — sqlx + SQLite

## Status
Accepted

## Context
CLAUDE.md Part 3 freezes the metadata store as an "embedded relational
store (Rust-native), insert-only/append-only at the schema level where
possible" but does not name a crate. This needs to be async-compatible
with the rest of the core (Tokio is already a workspace dependency for
the sidecar supervisor and Tauri's async command handlers), and it needs
to make INV-002/INV-004's insert-only/append-only contracts easy to
enforce at the repository layer (no `UPDATE`/`DELETE` on Table rows or
existing log entries, ever).

Candidates considered:
- **redb** — an embedded Rust KV store. Ruled out: it is a key-value
  store, not relational, so it doesn't satisfy the frozen "embedded
  relational store" wording at all — including it would have been
  quietly overriding Part 3 rather than implementing it.
- **rusqlite** — synchronous SQLite bindings. Simpler dependency tree,
  but every call blocks a thread; would need manual `spawn_blocking`
  wrapping throughout the repository layer to coexist with the rest of
  the async core.
- **sea-orm** — an async ORM built on sqlx. More codegen/abstraction over
  raw SQL than this project needs; the repository traits already provide
  the abstraction boundary the domain layer talks to, so a second
  abstraction layer underneath is redundant.
- **sqlx** — async, compile-time-checked SQL against SQLite, no ORM
  layer imposed on top. Matches the existing async posture and gives
  direct control over exactly which statements a repository issues,
  which matters for auditing that no repository implementation ever
  issues an `UPDATE`/`DELETE` against Table rows or existing log entries.

## Decision
Use `sqlx` with the `sqlite` and `runtime-tokio` features as the
metadata store for repository implementations (Table/Report metadata,
Recipes, Rulesets, Project/Dataset state). Content-addressed Parquet
(per Part 3) remains the store for actual table content; sqlx/SQLite is
for metadata and knowledge objects only.

## Consequences
- Repository trait methods in this pass are written as `async fn`,
  anticipating sqlx-backed implementations in a later task, rather than
  sync signatures that would need rework.
- Migrations will be plain SQL files run through sqlx's migrator, not
  ORM-managed schema — schema changes stay auditable as raw DDL.
- No actual database connection or schema exists yet in this pass;
  repository traits remain signature-only per this pass's scope.
