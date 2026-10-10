# ADR 0004: One Analysis engine, with an internal sidecar client

## Status
Accepted

## Context
The bootstrap brief for this pass asked for two engine trait
definitions — "Analysis (native)" and "Analysis (Python-backed
client)" — as separate entries in the engine list. But CLAUDE.md
INV-010 defines Analysis as a single engine with single responsibility:
"Analysis = executes Analysis Rulesets only (regardless of whether a
given procedure runs natively or via the Python sidecar — see
INV-029)," and the same invariant forbids any engine importing or
instantiating another engine — cross-engine work routes through the
command layer. Two structurally equal, co-equal `AnalysisEngine`-shaped
traits at the same layer would read as two engines, in tension with
that "one engine" framing, and having one call the other directly would
violate the no-engine-imports-an-engine rule.

## Decision
There is exactly one `AnalysisEngine` trait
(`crates/nikti-core/src/engines/analysis.rs`), with one method,
`analyse(content, definition, context: EngineContext)`. For procedures
that require the Python sidecar, its implementation delegates to
`PythonAnalysisClient` (`crates/nikti-core/src/sidecar/client.rs`) — a
plain IPC client trait, not a second engine. It lives under `sidecar/`,
not `engines/`, specifically so it doesn't read as a co-equal engine at
a glance. `EngineContext::Analysis { backend: ComputeBackend }` is what
tells a single `AnalysisEngine` implementation which path to take, per
INV-029's "first-class field of EngineContext," rather than the caller
picking between two different engine types.

## Consequences
- `AnalysisEngine::analyse` is the one command-layer entry point for
  both `analyse_native` and `analyse_python` commands; they differ only
  in the `ComputeBackend` they construct and pass in as part of
  `EngineContext`, not in which engine trait they call.
- `PythonAnalysisClient::call` takes `PythonSidecarStatus` by value (not
  buried in a larger context object it might not inspect), so an
  `AnalysisEngine` implementation cannot forward to the sidecar without
  looking at status first — a procedure requiring the sidecar is
  structurally un-runnable, not merely gated by convention, whenever
  status isn't `Ready` (INV-029).
- If a future task finds the single-trait shape too constraining (e.g.
  genuinely different capability surfaces between native and
  Python-backed procedures), that's a reason to revisit this ADR
  explicitly, not to quietly split `AnalysisEngine` back into two.
