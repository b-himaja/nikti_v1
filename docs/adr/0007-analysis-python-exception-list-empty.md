# ADR 0007: Analysis's Python-backed procedure exception list starts empty

## Status
Accepted

## Context
Part D of the Analysis Engine task prompt asked, for each of the 19
frozen Analysis Procedures, whether any *specific measurement within* a
procedure genuinely lacks a mature native Polars/Rust equivalent — and
explicitly forbade using the sidecar as a backdoor around the frozen
Future/Advanced exclusion list (C.3: no PCA, no hypothesis tests, no
model-based feature importance, etc., regardless of framing).

The measurements flagged in the task prompt as plausible gap candidates
were checked against the actual `polars` 0.44 crate surface (not assumed
from memory), consistent with the process already used for
Transformation (ADR 0006) and Validation:

1. **Pearson correlation (AP-008).** `polars::prelude::pearson_corr(a, b,
   ddof)` — already used natively in the Inspection engine's Column
   Relationships section. No gap.
2. **Spearman correlation (AP-008).** Not a distinct Polars primitive,
   but a mature native composition: Spearman's ρ is Pearson's r computed
   over each column's ranks. Polars' `Expr::rank()` is already enabled
   (`"rank"` feature, already in the workspace `Cargo.toml` from the
   Transformation port) and combines directly with `pearson_corr`. No
   gap — this is exactly the kind of "more code to write, not a missing
   capability" case INV-029 already excludes from qualifying.
3. **Cramér's V (AP-006).** Standard formula:
   `sqrt(chi2 / (n * min(rows-1, cols-1)))`, where `chi2` is Pearson's
   chi-squared statistic over the contingency table:
   `sum((observed - expected)^2 / expected)`. The contingency table
   itself is a native Polars `group_by`/`pivot` aggregation; the
   chi-squared/Cramér's-V arithmetic on top of that small, already-
   materialized table is plain scalar arithmetic in Rust, not a
   statistical library call. No gap.
4. **Eta squared, η² (AP-007).** Standard formula:
   `SS_between / SS_total` from one-way ANOVA, where both sums of
   squares are computable directly from native Polars group means/counts
   and the grand mean — again plain arithmetic over aggregates already
   produced by `group_by`, not a library gap.

None of the 19 procedures require p-values, confidence intervals, or a
distributional/hypothesis-testing library (`statrs` et al.) — the
frozen catalogue's own scope (C.3) explicitly excludes "statistical
hypothesis tests," so nothing in-scope ever needed one in the first
place. No procedure required anything beyond native Polars aggregation
expressions plus ordinary Rust arithmetic on the resulting (small,
already-collected) summary values.

## Decision
The Analysis engine's Python-backed procedure exception list is empty.
All 19 frozen procedures run 100% native Rust/Polars, which is also the
frozen default for every engine regardless of the exception list's
contents.

No `crates/nikti-core/src/engines/analysis/python_ops.rs` (or
equivalent registry) exists, because there is nothing to register.

## Consequences
- `inv029_exception_list_operations_are_explicit_and_enumerable`
  (CLAUDE.md Part 4) holds trivially for Analysis, same as it does for
  Transformation: an empty, explicit list is still an explicit list.
- `EngineContext::Analysis { backend: ComputeBackend }` (already
  scaffolded in `sidecar/compute_backend.rs` from the bootstrap pass)
  remains structurally present but is never actually exercised by any
  of the 19 procedures — `ComputeBackend::Native` is the only backend
  any procedure in this codebase ever resolves to. The field stays
  because a future procedure (outside the current frozen 19) could
  still need it; removing it now would be a bigger, unrelated structural
  change to a type other code already depends on.
- If a future Analysis procedure genuinely has no native equivalent for
  some measurement, that's a new, separate decision with its own review
  — not a reason to reopen this one, and never a route to implementing
  anything from the C.3 excluded list under a different name.
