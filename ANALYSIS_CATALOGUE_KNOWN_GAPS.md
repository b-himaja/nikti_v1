# Known Gaps Against the Frozen Analysis Procedure Catalogue

> Tracked deferral record, not a task prompt. This file exists so that
> "AP-013 is implemented" and "AP-018 is implemented" are never treated
> as fully true until the entries below are closed. Keep this file
> up to date whenever a deferred output is implemented — delete the row,
> don't just let it go stale.

## Why this file exists

The status audit found `AP-012`, `AP-013`, `AP-016`, and `AP-018` each
missing one named sub-table from their frozen catalogue definition. Two
were fixed as quick parameter-wiring follow-ups
(`regex_measure_table`, `target_by_segment_table`) — see
`ANALYSIS_PARAM_WIRING_PROMPT.md`. The two below require genuine new
algorithm work and are formally deferred rather than implemented under
time pressure or silently dropped.

## Deferred outputs

| Procedure | Missing output | Catalogue definition | Why deferred | What "done" looks like |
|---|---|---|---|---|
| AP-013 — Date/Time & Trend Explorer | `date_gap_table` | Gap detection: identifies ranges of missing dates/periods in the observed date column (distinct from `duration_summary_table`'s per-row start/end difference — see the AP-013 duration-semantics correction) | Gap detection requires a defined notion of "expected" date coverage (what granularity, what calendar) that hasn't been specified anywhere in the frozen spec beyond "gap count" as a measure — needs a design decision on the detection algorithm before implementation, not just engineering time | A real `date_gap_table` listing genuine missing-date ranges, backed by a documented gap-detection algorithm (e.g., expected-frequency inference or an explicit `time grain` parameter defining the expected cadence), with a test on data containing a known, deliberately-inserted gap |
| AP-018 — Dependency Candidate Explorer | `fd_exception_table` | Functional-dependency exception listing: rows that violate a candidate functional dependency (determinant → dependent) found elsewhere in the procedure's output | Functional-dependency *detection* (`dependency_candidate_table`) is already real; *exception listing* requires re-scanning the bound data for rows that violate a detected FD candidate, which is real work not yet scoped in detail (how many candidates get exception tables, what row-level detail is stored, how it interacts with the `DetailTableStore` write path from the evidence-integrity fix) | A real `fd_exception_table` populated for at least one detected FD candidate per run, using the same `DetailTableStore` path Validation's `AffectedRowsRef` now uses, with a test confirming exception rows genuinely violate the stated dependency |

## Standing rule while these remain open

`AP-013` and `AP-018` must continue to report their status honestly
wherever procedure completeness is surfaced (Report content, any future
catalogue-coverage dashboard, this project's own status-audit process) —
they are **Partial**, not **Real**, until both rows above are removed
from this file. Do not let the presence of *some* real output from
these two procedures (their other, already-real sub-tables) imply full
catalogue compliance in any status reporting.
