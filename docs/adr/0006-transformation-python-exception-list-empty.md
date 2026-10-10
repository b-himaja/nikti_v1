# ADR 0006: Transformation's Python-backed exception list starts empty

## Status
Accepted

## Context
CLAUDE.md's INV-029 was widened (see the file's Amendment log) to let any
engine, including Transformation, delegate individual operations to the
Python sidecar via an explicit, checked-in, per-engine exception list —
but only for operations with a genuine capability gap, never for
convenience, and never as a default.

Two separate investigations were run against the legacy Transformation
code (`legacy/datawrangler/python/engine/ops/{column,table,record}_ops.py`,
`formula_parser.py`) before writing any Rust:

1. **Empirical**: does the legacy code actually cross a process/IPC
   boundary for any Transformation operation? Grepped all five relevant
   files for `subprocess`, `multiprocessing`, sockets, HTTP clients,
   gRPC, Arrow Flight — zero matches. The only process boundary in the
   legacy repo is the *whole engine* running as one PyInstaller-packaged
   subprocess talking to a host app over newline-delimited JSON
   (`python-sidecar.spec`, `python/main.py`, `python/ipc/dispatcher.py`)
   — architecturally equivalent to Nikti's own `nikti_sidecar`, not
   evidence of any operation being individually offloaded further.
2. **Capability-gap**: for every operation in the legacy→Rust mapping
   table, does a mature native Polars/Rust equivalent exist? Checked
   against the actual `polars` 0.44 crate surface (not assumed from
   memory) — joins, aggregations, rolling windows, string/datetime
   namespaces, `cut`/`qcut` binning, sampling, etc. are all present
   natively, because Python Polars is itself a thin binding over this
   same Rust core. The one non-mechanical piece — the formula
   language — was hand-ported as a native parser (ADR 0005) rather than
   treated as a gap, since "needs more Rust code" is explicitly excluded
   from qualifying (INV-029: "not 'an equivalent exists but is more code
   to write'").

## Decision
The Transformation engine's Python-backed operation exception list is
empty. Every ported operation — all 22 column ops, 9 table ops, 6 record
ops, plus `derive_column`/`filter_table`'s formula parsing — runs 100%
native Rust/Polars, which is also the frozen default for every engine
regardless of the exception list's contents.

No `crates/nikti-core/src/engines/transformation/python_ops.rs` (or
equivalent registry) exists, because there is nothing to register.

## Consequences
- `inv029_exception_list_operations_are_explicit_and_enumerable`
  (CLAUDE.md Part 4) holds trivially for Transformation: an empty,
  explicit list is still an explicit list.
- Two Nikti-side additions go beyond strict legacy parity, both
  documented inline where implemented, not silently: `generate_uuid`
  accepts an optional deterministic seed (legacy's bare `uuid.uuid4()`
  per row made Recipe replay non-reproducible — a real gap against
  CLAUDE.md's determinism priority, not a legacy capability the port
  needed to preserve); `hash_column` and hand-index-based ops
  (`tag_rows`, `delete_rows`) use native Polars expressions/row-index
  operations instead of the legacy's Python-level per-row loops, since a
  native rewrite was available and Rule 8 requires native/deterministic
  execution wherever one exists.
- If a future Transformation operation genuinely has no native
  equivalent (e.g. a specific fuzzy-matching algorithm), that's a new,
  separate decision with its own review — not a reason to reopen this
  one.
