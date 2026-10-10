# ADR 0008: Definition-diagnostic severities

## Status
Accepted, under an acknowledged gap (see Context) — not a transcription
of a spec, because no such spec exists in this repository. This ADR is
authoritative going forward: any future task that touches a diagnostic
type's severity, adds a new diagnostic type, or changes the reasoning
behind an existing one must update this document in the same change,
not treat it as a historical record only.

## Context
The Definition Diagnostics engine (`crates/nikti-core/src/types/definition_diagnostics.rs`)
needed every one of its 13 diagnostic types assigned a severity —
`Error` (blocks `activate_version()`), `Warning`, or `Information` — in
order to decide which findings may block a ruleset version from
becoming Active and which may not.

No document defining a diagnostic-type vocabulary or severity model
exists anywhere in this repository. This was confirmed by an exhaustive
repo-wide search (all 13 diagnostic-type identifiers, plus phrases like
"Manager spec," "frozen Manager," "diagnostic vocabulary," and "Errors
block Save New Version") before the diagnostics engine was written —
zero matches anywhere, in any file type, outside unrelated vendored
third-party test names. This is consistent with the pattern already
documented in this codebase's own module doc-comments: both
`validation_ruleset.rs` and `analysis_ruleset.rs` state that their
source "governing task prompt" document was itself unavailable at
implementation time.

The 13 diagnostic-type *names* below come from the task prompt that
commissioned the diagnostics engine — that prompt is the only place
they are written down anywhere. The *severity* assigned to each one,
and the reasoning behind it, was decided directly against this
codebase's actual type model and execution semantics, not read off a
spec. That reasoning previously lived only in inline code comments in
`definition_diagnostics.rs` and in that task's own chat-turn Output. It
is reproduced here in full — not summarized — so it survives independent
of any one conversation, and so a future change to a severity has a
single place to update instead of drifting inline comments out of sync
with each other.

See Part C's recommendation below for what closes this gap for real.

## Decision

| Diagnostic type | Severity | Reasoning |
|---|---|---|
| `missing_required_field` | Error | An unnamed rule/procedure can't be identified in a report, a log entry, or a UI list — there is no scenario where a blank name is anything but a defect, and no way for it to resolve itself later. |
| `duplicate_rule_id` | Error | A shared id breaks referential integrity — report detail refs, dependency resolution by name, and any future log/report cross-reference all assume ids are unique within a version. There is no scenario where two items sharing an id is anything but a defect. |
| `duplicate_procedure_id` | Error | Same reasoning as `duplicate_rule_id`, for Analysis's `AnalysisProcedure.procedure_id` — this is the same shared check (`diagnose_items`'s `seen_ids` loop), not an independently-reasoned twin. |
| `unsupported_rule_type` | Error | A rule using an unsupported `function_id` (Validation's `ValidationFunctionParams::Unknown`) is guaranteed to resolve to `RuleOutcome::NotExecutable` at execution time, every run, regardless of data or parameters. Catching it before activation prevents a version that is already known to contain guaranteed-dead entries. |
| `unsupported_procedure_type` | Error | Same reasoning as `unsupported_rule_type`, for Analysis. **Structurally unreachable today**: `AnalysisProcedureParams` is an exhaustive, closed enum with no catch-all variant (unlike Validation's `Unknown`), so no constructible value can ever fail this check. The check exists for uniformity with Validation and for future-proofing (if a catch-all variant is ever added to the Analysis catalogue, this check becomes live automatically, with no code change needed at the call site) — not because a real fixture can trigger it today. |
| `invalid_parameter` | Error | A function/procedure's own configured parameter values are semantically empty or degenerate in a way that guarantees a degenerate or failed computation at execution time regardless of the data it eventually runs against — e.g. AP-012's `regex_list: Some(vec![])` (configured, but with nothing to match), an unparseable regex pattern, or an inverted numeric range (`min > max` in `BetweenValues`/`LengthBetween`/`DistinctCountBetween`). This is the same "guaranteed deterministic failure, independent of the target object" logic as `invalid_expression` and `missing_required_column`/`missing_required_role`, applied to a function/procedure's own payload fields rather than its target binding. Deliberately does **not** fire on parameters where an empty value is a real, intentional feature verified against the actual implementation — `ColumnPropertyExplorer`, `MissingnessStructureExplorer`, `RepetitionDuplicateStructureExplorer`, and `DependencyCandidateExplorer` all treat an empty column list as "use all columns," and `TextMeasurementExplorer`'s empty `keywords` legitimately means "no keyword search." |
| `missing_required_column` | Error | A column-bound target (`TargetKind::ColumnBound` — Validation's `Column`/`ColumnSet`, Analysis's `Column`/`ColumnSet`) that names zero columns cannot resolve to anything at execution time; there is no runtime configuration that could make this work. Deliberately keyed off `target_kind()`, not `binding_mode`, because both engines' `table_scope()`-style constructors leave `binding_mode` at its `FixedColumnName` default even when zero columns are bound (verified against the real `AnalysisProcedureTarget::table_scope()`, used by the mandatory AP-001 and by AP-019) — `binding_mode` alone is not a reliable signal for "this needs columns." |
| `missing_required_role` | Error | Same reasoning as `missing_required_column`, for Analysis's role-bound target kind (`AnalysisTargetType::RoleSet`) and its `target_roles` list. Validation has no role concept at all — no `ValidationTargetType` variant ever maps to `TargetKind::RoleBound`, so no `ValidationRule` can ever report this diagnostic; this is a structural fact of Validation's object model, not a special case carved out in the check itself. |
| `invalid_expression` | Error | The one raw-expression field each engine has (Validation's `ExpressionTrue.formula`, Analysis's `DerivedMeasureDefinition.formula`) is checked with `FormulaParser::compile(formula, None)` — schema-blind, syntax- and function-name-only, since no `Schema` exists at ruleset-authoring time (rulesets are reusable knowledge, run against many different Tables/Datasets over time — INV-006 — so column existence can't be validated here, and `FormulaParser::compile` skips that check entirely when given `None`, per its own doc comment). A syntactically malformed expression is still guaranteed to fail at execution time regardless of which object it eventually runs against, so this is unambiguously blocking despite the schema-blind scope. |
| `unresolved_dependency` | **Warning** | `ProcedureDependency.required_output` (Analysis only — Validation has no dependency concept and always reports zero of these) is a free-form string naming an output, not a structural procedure-id edge. Several procedures produce some of their output tables only *conditionally* on their own configured parameters (AP-012's `regex_measure_table`, AP-013's `duration_summary_table`, AP-016's `target_by_segment_table`) — whether one of those conditional tables will exist can't be determined without re-running that procedure's own parameter logic, which this static check deliberately does not do (see `known_output_names`'s doc comment: it lists only *unconditional* outputs). Failing to find a statically-known producer therefore does **not** prove the dependency can never be satisfied at runtime — a procedure's own runtime configuration can make a conditional output appear in a way that can't be proven absent statically. That genuine uncertainty is why this is a Warning, not an Error. |
| `circular_dependency` | Error | A cycle is a different situation from a single unresolved dependency: if procedure A requires an output only B is known to produce, and B requires an output only A is known to produce, execution is strictly ordered (procedures run once, in declared sequence, never re-entrant) — whichever of the two runs first can never see the other's not-yet-produced output. At least one side is therefore **guaranteed** to hit its `OnMissingDependency` policy on every single run, regardless of parameters or data. That is a deterministic, provable failure (unlike a single unresolved dependency's genuine runtime uncertainty), so `circular_dependency` is Error. |
| `incompatible_target_type` | **Warning** | Fires when a whole-table-scoped item (`TargetKind::TableScope` — Validation's `Table`, Analysis's `TableScope`) exists in a version whose compatible-target-types list excludes the "Table" object kind. This is a naming-consistency signal, not a proven guaranteed execution failure: nothing in either engine's actual type model makes a table-scope check (e.g. row-count, whole-object uniqueness) structurally unable to run against a Dataset instead of a Table — both are just row-based content at that level of granularity. Kept as a Warning specifically because no case in this codebase's real object model demonstrates a genuine hard incompatibility here; if one is ever found, this severity should be revisited (and this ADR updated, per its Status note). |
| `unsupported_catalogue_version` | Error | An engine running a ruleset version pinned to a catalogue/schema version string it doesn't recognize (`ruleset_schema_version`, `function_catalogue_version`, `analysis_procedure_catalogue_version`, `analysis_engine_min_version`) could silently execute against the wrong contract, which is a direct INV-001 provenance risk — provenance claims are only meaningful if the recorded catalogue version is one the current engine code actually understands. The supported-version allowlist (`SUPPORTED_RULESET_SCHEMA_VERSIONS` et al., in `validation_ruleset.rs`/`analysis_ruleset.rs`) is a small, explicit list matching the literal version strings this codebase's own real fixtures already use (`"1.0"`, `"1.0.0"`, `"0.1.0"`) — there being no external spec to source a "currently supported" list from otherwise. |

Two severities deviate from the otherwise-uniform "Error" default —
`unresolved_dependency` and `incompatible_target_type` — and both
deviate for the same underlying reason: each is a case where this
codebase's actual object model cannot prove the flagged condition is a
guaranteed failure, only that it might be one. Every other diagnostic
type is Error specifically because it *is* provably guaranteed to fail
at execution time, independent of data or further configuration.

## Consequences
- `definition_diagnostics.rs`'s inline comments at each severity
  assignment now point to this ADR by number instead of restating the
  full reasoning inline — this document is the source of truth for
  *why* a severity is what it is; the code comment is a pointer, not a
  second copy that can silently drift out of sync with this one (the
  same "one source of truth" discipline already applied to the shared
  `Diagnosable` trait itself, so the check logic isn't duplicated
  between engines either).
- Changing a severity, or the reasoning behind one, requires editing
  this ADR in the same change — not just the code comment, and not
  leaving this document to describe a decision that no longer matches
  the code.
- **This does not close the underlying gap.** No spec document exists
  in this repository to check this ADR's calls against — see Part C of
  the task that produced this ADR, reproduced here since it is exactly
  as true of this document as it was of the diagnostics engine itself:
  every future prompt or task that says "per the frozen spec" for
  Validation/Analysis diagnostics is making the same unverifiable claim.
  If real spec text is ever committed to this repository (e.g. under a
  `docs/specs/` directory), a follow-up task should audit every ADR and
  inline "per the spec" comment written under this gap — including this
  one — and confirm each actually matches the real document. This ADR
  may turn out to have made a different call than an actual spec would
  have; that would be a reason to revise it, not evidence that this
  decision was made carelessly.
