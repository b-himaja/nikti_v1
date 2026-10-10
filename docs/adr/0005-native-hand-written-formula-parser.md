# ADR 0005: Native, hand-written formula parser (no grammar-crate dependency)

## Status
Accepted

## Context
`derive_column` and `filter_table`'s `condition` in the legacy prototype
(`legacy/datawrangler/python/engine/formula_parser.py`) both depend on a
small formula language — arithmetic, comparisons, `AND`/`OR`/`NOT`,
column references, string/number literals, and a fixed whitelist of
functions (`ABS`, `ROUND`, `IF`, ...) — compiled to a Polars expression
tree via a `lark`-based grammar (Earley parser, no `eval`/`exec`
anywhere).

Porting this needs *some* way to parse formula strings into
`polars::prelude::Expr` in Rust. Two shapes were considered:
- **A grammar/parser-generator crate** (e.g. `pest`, `nom`, `lalrpop`) —
  more standard for larger or evolving grammars, but a new dependency
  decision, and more machinery than this grammar's actual size warrants.
- **A hand-written recursive-descent parser** — the legacy grammar is
  small and fully specified already (arithmetic precedence, a closed
  function whitelist, no extensibility requirement); a recursive-descent
  parser matching it directly is well within normal hand-written-parser
  territory and needs nothing beyond the standard library.

## Decision
Hand-write a recursive-descent parser
(`crates/nikti-core/src/engines/transformation/formula.rs`) that tokenizes
and parses directly to `Expr`, matching the legacy grammar's operator
precedence, function whitelist, and semantics (including its null-safe
division/modulo and its literal-only second argument for
`ROUND`/`LOG`/`PCTCHANGE`, which the legacy parser only ever supported by
reading `.meta["value"]` straight off a NUMBER token in the parse tree).
No new parsing-library dependency was added.

One deliberate deviation, flagged rather than silently resolved: the
legacy `CNAME` terminal allows embedded spaces in a *bare* (unbracketed)
column reference, which collides ambiguously with the `AND`/`OR`/`NOT`
keywords and is only resolved by Lark's Earley `ambiguity="resolve"`
heuristics — not independently verifiable without Lark itself. This
parser requires brackets (`[sales tax]`) for any column name containing
a space or matching a reserved word; bare identifiers are restricted to
`[a-zA-Z_][a-zA-Z0-9_]*`.

One deliberate bug fix, not a silent behavior change: `MOD` was listed in
the legacy `SUPPORTED_FUNCTIONS` but had no matching arm in its
`func_call` if/elif chain, so `MOD(a, b)` always raised "Unimplemented
function: MOD" despite being advertised as supported. Here it's wired to
the same null-safe-modulo logic as the `%` operator.

## Consequences
- `derive_column` and `filter_table`'s `condition` are fully native and
  do not qualify for the Python-backed exception list under INV-029 —
  parsing a small, fully-specified grammar is squarely "more code to
  write in Rust," not a genuine capability gap (see ADR 0006).
- Verified against real Polars execution, not just compiled: see
  `crates/nikti-core/src/engines/transformation/formula_tests.rs`.
- If the formula language grows substantially (more operators, nested
  scoping, user-definable functions), that's grounds to revisit this
  ADR and reconsider a parser-generator crate — not to keep hand-rolling
  extensions to this parser indefinitely.
