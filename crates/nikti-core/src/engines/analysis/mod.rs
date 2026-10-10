//! Analysis Engine (Analysis Engine Runtime Contract v1.0, as summarized
//! in the governing task prompt — the full source document was
//! unavailable in this repository at implementation time; see the final
//! compliance report). INV-012: executes Analysis Rulesets only. INV-010:
//! never calls another engine, including Validation. Part A (binding):
//! **Analysis Procedure**, never "Analysis Rule."
//!
//! Amendment 2 (binding correction): `AnalysisResult` is **not** immutable
//! evidence — it is a transient, deterministic execution output consumed
//! by the Report Engine and then done. The Analysis Report (+ Execution
//! Metadata) is the permanent evidence record. Do not add Category-A
//! no-write-method guarantees to `AnalysisResult` — see
//! `tests::analysis_result_is_not_subject_to_category_a_immutability`,
//! which documents this explicitly so a future reader doesn't "fix" it.

pub mod procedures;
pub mod python_ops;
#[cfg(test)]
mod tests;

use polars::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::engines::transformation::formula::FormulaParser;
use crate::types::common::{ProjectId, Timestamp};
use crate::types::{AnalysisRulesetVersion, SampleMode};

use super::content::{DatasetContent, TableContent};

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error("request error: {0}")]
    Request(String),
}

/// E.3: request-level vs. item-level error categories — a `RequestError`
/// stops execution before any procedure runs; every other category
/// affects at most the one procedure it's attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AnalysisErrorCategory {
    RequestError,
    TargetError,
    RulesetError,
    BindingError,
    DependencyError,
    ProcedureExecutionError,
    EngineError,
    ResourceError,
    ReportRequestError,
}

/// E.2: Analysis's own engine-state vocabulary — deliberately not
/// Validation's (an extra `SubmittingReportRequest` stage is named here
/// that Validation's runtime contract never named).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExecutionStatus {
    Queued,
    Preparing,
    Executing,
    Aggregating,
    SubmittingReportRequest,
    Completed,
    CompletedWithWarnings,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProcedureStatus {
    Completed,
    CompletedWithWarnings,
    Skipped,
    NotExecutable,
    ExecutionError,
}

#[derive(Debug, Clone, Serialize)]
pub struct Measure {
    pub measure_name: String,
    pub value: f64,
    pub unit: Option<String>,
    pub input_columns: Vec<String>,
    pub method: String,
    pub missing_policy: String,
    pub denominator: Option<f64>,
}

/// E.5: closed finding-type vocabulary — do not add variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FindingType {
    MeasuredObservation,
    AssociationCandidate,
    DependencyCandidate,
    BoundaryCandidate,
    RetentionEvidence,
    RowAttentionEvidence,
    ProcedureWarning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub finding_id: Uuid,
    pub finding_type: FindingType,
    pub statement: String,
    pub evidence_refs: Vec<String>,
    pub affected_columns: Vec<String>,
    pub affected_rows_ref: Option<String>,
}

/// Reference, not embedded data — same discipline as Inspection's
/// sampling-affected sections and Validation's `AffectedRowsRef`: row
/// count and schema only, never bulk row values.
#[derive(Debug, Clone, Serialize)]
pub struct OutputTableRef {
    pub name: String,
    pub reference: String,
    pub row_count: u64,
    pub columns: Vec<String>,
}

/// C.4: only the allowed visual-output types. This carries the
/// *underlying data reference* for a chart (bucketed counts, coordinate
/// pairs, etc.) — rendering it is a Report Viewer concern, out of scope
/// here, same as Inspection's Distribution Views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VisualOutputType {
    Histogram,
    BoxPlot,
    ScatterPlot,
    BarChart,
    GroupedBarChart,
    LineChart,
    Heatmap,
    ParetoChart,
    MatrixView,
    SummaryTable,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualOutputRef {
    pub visual_type: VisualOutputType,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcedureResult {
    pub procedure_result_id: Uuid,
    pub procedure_id: Uuid,
    pub procedure_type: String,
    pub procedure_name: String,
    pub status: ProcedureStatus,
    pub input_scope: Vec<String>,
    pub parameters_used: String,
    pub measures: Vec<Measure>,
    pub output_table_refs: Vec<OutputTableRef>,
    pub visual_output_refs: Vec<VisualOutputRef>,
    pub detail_refs: Vec<String>,
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisSummary {
    pub total_procedures: u64,
    pub completed_procedures: u64,
    pub completed_with_warnings: u64,
    pub skipped_procedures: u64,
    pub not_executable_procedures: u64,
    pub execution_error_procedures: u64,
    pub total_output_tables: u64,
    pub total_visual_outputs: u64,
    pub total_findings: u64,
    pub total_warnings: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AnalysisTargetObjectType {
    Table,
    Dataset,
}

/// Amendment 4-equivalent sampling disclosure (B.4: "apply identically"
/// to Inspection's discipline). A local copy rather than reusing
/// `inspection::SamplingDisclosure` directly — engines stay decoupled
/// from each other's internal result types (INV-010's spirit), even
/// though this is a type reuse, not a cross-engine call.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisSamplingDisclosure {
    pub mode: SampleMode,
    pub sample_size: Option<u64>,
    pub seed: Option<u64>,
    pub full_row_count: u64,
    pub sampled_row_count: u64,
    pub affected_procedures: Vec<String>,
}

/// E.4 (Amendment 2, binding): transient — NOT Category-A evidence. See
/// the module doc comment.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub analysis_result_id: Uuid,
    pub execution_id: Uuid,
    pub engine_id: String,
    pub execution_status: ExecutionStatus,
    pub target_object_type: AnalysisTargetObjectType,
    pub target_object_id: Uuid,
    pub project_id: Option<ProjectId>,
    pub analysis_ruleset_id: Uuid,
    pub analysis_ruleset_version_id: Uuid,
    pub procedure_results: Vec<ProcedureResult>,
    pub analysis_summary: AnalysisSummary,
    pub findings: Vec<Finding>,
    pub output_tables: Vec<OutputTableRef>,
    pub visual_outputs: Vec<VisualOutputRef>,
    pub detail_refs: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    /// The fatal request-level error, if any (E.3's `request_error` and
    /// siblings) — set only when execution stopped before any procedure
    /// ran. Distinct from any individual procedure's `ExecutionError`
    /// status, which never aborts the run.
    pub fatal_error: Option<(AnalysisErrorCategory, String)>,
    pub sampling: Option<AnalysisSamplingDisclosure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisExecutionMetadata {
    pub execution_id: Uuid,
    pub target_object_type: AnalysisTargetObjectType,
    pub target_object_id: Uuid,
    pub project_id: Option<ProjectId>,
    pub analysis_ruleset_id: Uuid,
    pub analysis_ruleset_version_id: Uuid,
    pub ruleset_hash: String,
    pub analysis_procedure_catalogue_version: String,
    pub engine_version: crate::types::common::EngineVersion,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
    pub actor: String,
}

/// C.7/E.4: which log a run routes to, structurally derived from which
/// `AnalysisTarget` variant was used — mirrors Validation's
/// `LogRoute`/`log_route_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRoute {
    TableEventLog,
    ProjectOperationLog { project_id: ProjectId },
}

pub fn log_route_for(metadata: &AnalysisExecutionMetadata) -> LogRoute {
    match metadata.target_object_type {
        AnalysisTargetObjectType::Table => LogRoute::TableEventLog,
        AnalysisTargetObjectType::Dataset => {
            LogRoute::ProjectOperationLog { project_id: metadata.project_id.expect("project_id is mandatory for a Dataset target") }
        }
    }
}

/// B.3: single-target only — there is no variant, and none may ever be
/// added, that binds two Tables/Datasets to one Analysis run.
pub enum AnalysisTarget {
    Table(TableContent),
    Dataset { content: DatasetContent, project_id: ProjectId },
}

impl AnalysisTarget {
    fn frame(&self) -> LazyFrame {
        match self {
            AnalysisTarget::Table(c) => c.frame.clone(),
            AnalysisTarget::Dataset { content, .. } => content.frame.clone(),
        }
    }

    fn object_id(&self) -> Uuid {
        match self {
            AnalysisTarget::Table(c) => c.table_id,
            AnalysisTarget::Dataset { content, .. } => content.dataset_id,
        }
    }

    fn object_type(&self) -> AnalysisTargetObjectType {
        match self {
            AnalysisTarget::Table(_) => AnalysisTargetObjectType::Table,
            AnalysisTarget::Dataset { .. } => AnalysisTargetObjectType::Dataset,
        }
    }

    fn project_id(&self) -> Option<ProjectId> {
        match self {
            AnalysisTarget::Table(_) => None,
            AnalysisTarget::Dataset { project_id, .. } => Some(*project_id),
        }
    }
}

/// INV-010: analysis only — never calls another engine (including
/// Validation), never writes to a Table or Dataset, never trains or
/// persists a model.
pub trait AnalysisEngine {
    /// `definition` is always a specific, already-resolved
    /// `AnalysisRulesetVersion` — no overload accepts an unresolved
    /// "latest" `AnalysisRuleset`.
    fn analyse(
        &self,
        target: AnalysisTarget,
        analysis_ruleset_id: Uuid,
        definition: &AnalysisRulesetVersion,
        actor: String,
        started_at: Timestamp,
    ) -> (AnalysisResult, AnalysisExecutionMetadata, Vec<(String, DataFrame)>) {
        let _ = (target, analysis_ruleset_id, definition, actor, started_at);
        todo!()
    }
}

pub struct NativeAnalysisEngine;

impl AnalysisEngine for NativeAnalysisEngine {
    fn analyse(
        &self,
        target: AnalysisTarget,
        analysis_ruleset_id: Uuid,
        definition: &AnalysisRulesetVersion,
        actor: String,
        started_at: Timestamp,
    ) -> (AnalysisResult, AnalysisExecutionMetadata, Vec<(String, DataFrame)>) {
        analyse_standard(target, analysis_ruleset_id, definition, actor, started_at, "0.1.0", &|_| false)
    }
}

fn empty_summary() -> AnalysisSummary {
    AnalysisSummary {
        total_procedures: 0,
        completed_procedures: 0,
        completed_with_warnings: 0,
        skipped_procedures: 0,
        not_executable_procedures: 0,
        execution_error_procedures: 0,
        total_output_tables: 0,
        total_visual_outputs: 0,
        total_findings: 0,
        total_warnings: 0,
    }
}

/// Internal, independently testable entry point — implements E.1's
/// eleven-stage pipeline explicitly (two more stages than Validation's:
/// Resolve Scope, Resolve Procedure Dependencies).
#[allow(clippy::too_many_arguments)]
pub fn analyse_standard(
    target: AnalysisTarget,
    analysis_ruleset_id: Uuid,
    ruleset_version: &AnalysisRulesetVersion,
    actor: String,
    started_at: Timestamp,
    engine_version: &str,
    cancel: &dyn Fn(usize) -> bool,
) -> (AnalysisResult, AnalysisExecutionMetadata, Vec<(String, DataFrame)>) {
    let execution_id = Uuid::new_v4();
    let analysis_result_id = Uuid::new_v4();

    let metadata_base = || AnalysisExecutionMetadata {
        execution_id,
        target_object_type: target.object_type(),
        target_object_id: target.object_id(),
        project_id: target.project_id(),
        analysis_ruleset_id,
        analysis_ruleset_version_id: ruleset_version.analysis_ruleset_version_id(),
        ruleset_hash: ruleset_version.ruleset_hash().to_string(),
        analysis_procedure_catalogue_version: ruleset_version.analysis_procedure_catalogue_version().to_string(),
        engine_version: crate::types::common::EngineVersion { rust_core: engine_version.to_string(), python_env_hash: None },
        started_at,
        completed_at: started_at,
        actor: actor.clone(),
    };

    let fail_fast = |category: AnalysisErrorCategory, message: String| {
        let result = AnalysisResult {
            analysis_result_id,
            execution_id,
            engine_id: "analysis_engine".to_string(),
            execution_status: ExecutionStatus::Failed,
            target_object_type: target.object_type(),
            target_object_id: target.object_id(),
            project_id: target.project_id(),
            analysis_ruleset_id,
            analysis_ruleset_version_id: ruleset_version.analysis_ruleset_version_id(),
            procedure_results: Vec::new(),
            analysis_summary: empty_summary(),
            findings: Vec::new(),
            output_tables: Vec::new(),
            visual_outputs: Vec::new(),
            detail_refs: Vec::new(),
            warnings: Vec::new(),
            errors: vec![message.clone()],
            fatal_error: Some((category, message)),
            sampling: None,
        };
        (result, metadata_base(), Vec::new())
    };

    // --- 1. Validate Request --- B.3: target-kind compatibility.
    let target_kind_compatible = ruleset_version.compatible_target_types().iter().any(|t| match (t, target.object_type()) {
        (crate::types::AnalysisCompatibleTargetType::Table, AnalysisTargetObjectType::Table) => true,
        (crate::types::AnalysisCompatibleTargetType::Dataset, AnalysisTargetObjectType::Dataset) => true,
        _ => false,
    });
    if !target_kind_compatible {
        return fail_fast(AnalysisErrorCategory::RequestError, "ruleset version is not compatible with this target's object type".to_string());
    }

    // --- 2. Load Target Read-Only ---
    let full_df = match target.frame().collect() {
        Ok(df) => df,
        Err(e) => return fail_fast(AnalysisErrorCategory::TargetError, format!("target could not be loaded: {e}")),
    };
    let full_row_count = full_df.height() as u64;
    if let Some(min) = ruleset_version.target_contract().minimum_row_count {
        if full_row_count < min {
            return fail_fast(AnalysisErrorCategory::TargetError, format!("target has {full_row_count} rows, below the ruleset's minimum of {min}"));
        }
    }

    // --- 3. Load Analysis Ruleset Version --- (already `ruleset_version`)

    // --- 4. Resolve Scope --- included/excluded columns, row filter,
    // sampling. Column selection failures and an invalid filter formula
    // are `RulesetError`s (the ruleset's own scope definition doesn't
    // resolve against this target) — request-level, not per-procedure.
    let scope = &ruleset_version.default_scope();
    let mut scoped_lf = full_df.clone().lazy();
    if let Some(expr) = &scope.row_filter_expression {
        let schema = full_df.schema();
        match FormulaParser::compile(expr, Some(&schema)) {
            Ok(filter_expr) => scoped_lf = scoped_lf.filter(filter_expr.cast(DataType::Boolean)),
            Err(e) => return fail_fast(AnalysisErrorCategory::RulesetError, format!("row_filter_expression is invalid: {e}")),
        }
    }
    let mut scoped_df = match scoped_lf.collect() {
        Ok(df) => df,
        Err(e) => return fail_fast(AnalysisErrorCategory::RulesetError, format!("scope resolution failed: {e}")),
    };
    if !scope.included_columns.is_empty() {
        match scoped_df.select(scope.included_columns.iter().cloned()) {
            Ok(df) => scoped_df = df,
            Err(e) => return fail_fast(AnalysisErrorCategory::RulesetError, format!("included_columns do not resolve against this target: {e}")),
        }
    }
    if !scope.excluded_columns.is_empty() {
        let keep: Vec<String> = scoped_df.get_column_names().into_iter().map(|s| s.to_string()).filter(|c| !scope.excluded_columns.contains(c)).collect();
        scoped_df = match scoped_df.select(keep) {
            Ok(df) => df,
            Err(e) => return fail_fast(AnalysisErrorCategory::RulesetError, format!("excluded_columns resolution failed: {e}")),
        };
    }

    let sampling = match scope.sample_policy.mode {
        SampleMode::FullData => None,
        SampleMode::RowLimit | SampleMode::RandomSample | SampleMode::StratifiedSample => {
            let n = (scope.sample_policy.sample_size.unwrap_or(scoped_df.height() as u64) as usize).min(scoped_df.height());
            let seed = scope.sample_policy.random_seed;
            let sampled = match scoped_df.sample_n_literal(n, false, matches!(scope.sample_policy.mode, SampleMode::RandomSample | SampleMode::StratifiedSample), seed) {
                Ok(s) => s,
                Err(e) => return fail_fast(AnalysisErrorCategory::RulesetError, format!("sampling failed: {e}")),
            };
            let disclosure = AnalysisSamplingDisclosure {
                mode: scope.sample_policy.mode,
                sample_size: Some(n as u64),
                seed,
                full_row_count,
                sampled_row_count: sampled.height() as u64,
                affected_procedures: ruleset_version.procedures().iter().filter(|p| p.enabled()).map(|p| p.procedure_type().to_string()).collect(),
            };
            scoped_df = sampled;
            Some(disclosure)
        }
    };

    let schema_columns: std::collections::HashSet<String> = scoped_df.get_column_names().into_iter().map(|s| s.to_string()).collect();

    // --- 5. Resolve Runtime Bindings --- per-procedure column existence.
    // --- 6. Resolve Procedure Dependencies --- tracked via
    // `satisfied_outputs` as procedures complete, in declared order
    // (AP-001 always first, per B.7).
    let mut satisfied_outputs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut procedure_results = Vec::new();
    // The real content behind every `OutputTableRef` above, collected
    // across all procedures — the command layer writes these to real
    // storage (`DetailTableStore`) after this function returns. Engines
    // compute; storage I/O is a command/repository-layer job (INV-008).
    let mut output_table_content: Vec<(String, DataFrame)> = Vec::new();

    // --- 7. Execute Analysis Procedures ---
    for (idx, procedure) in ruleset_version.procedures().iter().enumerate() {
        let procedure_result_id = Uuid::new_v4();
        let base = |status: ProcedureStatus, warnings: Vec<String>, errors: Vec<String>| ProcedureResult {
            procedure_result_id,
            procedure_id: procedure.procedure_id(),
            procedure_type: procedure.procedure_type().to_string(),
            procedure_name: procedure.procedure_name().to_string(),
            status,
            input_scope: procedure.target().target_columns.clone(),
            parameters_used: format!("{:?}", procedure.params()),
            measures: Vec::new(),
            output_table_refs: Vec::new(),
            visual_output_refs: Vec::new(),
            detail_refs: Vec::new(),
            findings: Vec::new(),
            warnings,
            errors,
        };

        if !procedure.enabled() {
            procedure_results.push(base(ProcedureStatus::Skipped, vec!["procedure is disabled".to_string()], Vec::new()));
            continue;
        }
        if cancel(idx) {
            procedure_results.push(base(ProcedureStatus::Skipped, vec!["cancelled".to_string()], Vec::new()));
            continue;
        }

        let missing_columns: Vec<&String> = procedure.target().target_columns.iter().filter(|c| !schema_columns.contains(*c)).collect();
        if !missing_columns.is_empty() {
            procedure_results.push(base(
                ProcedureStatus::NotExecutable,
                Vec::new(),
                vec![format!("target column(s) not present after scope resolution: {}", missing_columns.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))],
            ));
            continue;
        }

        let mut dependency_failed = false;
        for dep in procedure.dependency_refs() {
            let satisfied = match dep.required_output.as_ref() {
                Some(output) => satisfied_outputs.contains(output),
                None => true,
            };
            if !satisfied {
                match dep.on_missing {
                    crate::types::OnMissingDependency::NotExecutable => {
                        procedure_results.push(base(ProcedureStatus::NotExecutable, Vec::new(), vec!["required dependency output missing".to_string()]));
                        dependency_failed = true;
                        break;
                    }
                    crate::types::OnMissingDependency::SkipWithWarning => {
                        procedure_results.push(base(ProcedureStatus::Skipped, vec!["required dependency output missing — skipped".to_string()], Vec::new()));
                        dependency_failed = true;
                        break;
                    }
                    crate::types::OnMissingDependency::ExecutionError => {
                        procedure_results.push(base(ProcedureStatus::ExecutionError, Vec::new(), vec!["required dependency output missing".to_string()]));
                        dependency_failed = true;
                        break;
                    }
                }
            }
        }
        if dependency_failed {
            continue;
        }

        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            procedures::execute_procedure(&scoped_df, procedure, procedure_result_id, &procedure_results)
        })) {
            Ok(Ok(outcome)) => {
                for t in &outcome.output_table_refs {
                    satisfied_outputs.insert(t.name.clone());
                }
                output_table_content.extend(outcome.output_table_data);
                let status = if !outcome.warnings.is_empty() { ProcedureStatus::CompletedWithWarnings } else { ProcedureStatus::Completed };
                procedure_results.push(ProcedureResult {
                    procedure_result_id,
                    procedure_id: procedure.procedure_id(),
                    procedure_type: procedure.procedure_type().to_string(),
                    procedure_name: procedure.procedure_name().to_string(),
                    status,
                    input_scope: procedure.target().target_columns.clone(),
                    parameters_used: format!("{:?}", procedure.params()),
                    measures: outcome.measures,
                    output_table_refs: outcome.output_table_refs,
                    visual_output_refs: outcome.visual_output_refs,
                    detail_refs: Vec::new(),
                    findings: outcome.findings,
                    warnings: outcome.warnings,
                    errors: Vec::new(),
                });
            }
            Ok(Err(e)) => {
                procedure_results.push(base(ProcedureStatus::ExecutionError, Vec::new(), vec![format!("execution error: {e}")]));
            }
            Err(_) => {
                procedure_results.push(base(ProcedureStatus::ExecutionError, Vec::new(), vec!["execution error: procedure evaluation panicked".to_string()]));
            }
        }
    }

    // --- 8. Aggregate Procedure Results ---
    let cancelled = procedure_results.iter().any(|r| r.status == ProcedureStatus::Skipped && r.warnings.iter().any(|w| w == "cancelled"));
    let mut summary = empty_summary();
    summary.total_procedures = ruleset_version.procedures().len() as u64;
    let mut all_findings = Vec::new();
    let mut all_output_tables = Vec::new();
    let mut all_visual_outputs = Vec::new();
    let mut all_warnings = Vec::new();
    let mut all_errors = Vec::new();
    for r in &procedure_results {
        match r.status {
            ProcedureStatus::Completed => summary.completed_procedures += 1,
            ProcedureStatus::CompletedWithWarnings => summary.completed_with_warnings += 1,
            ProcedureStatus::Skipped => summary.skipped_procedures += 1,
            ProcedureStatus::NotExecutable => summary.not_executable_procedures += 1,
            ProcedureStatus::ExecutionError => summary.execution_error_procedures += 1,
        }
        summary.total_output_tables += r.output_table_refs.len() as u64;
        summary.total_visual_outputs += r.visual_output_refs.len() as u64;
        summary.total_findings += r.findings.len() as u64;
        summary.total_warnings += r.warnings.len() as u64;
        all_findings.extend(r.findings.iter().cloned());
        all_output_tables.extend(r.output_table_refs.iter().cloned());
        all_visual_outputs.extend(r.visual_output_refs.iter().cloned());
        all_warnings.extend(r.warnings.iter().cloned());
        all_errors.extend(r.errors.iter().cloned());
    }

    let execution_status = if cancelled {
        ExecutionStatus::Cancelled
    } else if summary.execution_error_procedures > 0 {
        ExecutionStatus::CompletedWithWarnings
    } else if summary.completed_with_warnings > 0 {
        ExecutionStatus::CompletedWithWarnings
    } else {
        ExecutionStatus::Completed
    };

    // --- 9. Create Analysis Result ---
    let result = AnalysisResult {
        analysis_result_id,
        execution_id,
        engine_id: "analysis_engine".to_string(),
        execution_status,
        target_object_type: target.object_type(),
        target_object_id: target.object_id(),
        project_id: target.project_id(),
        analysis_ruleset_id,
        analysis_ruleset_version_id: ruleset_version.analysis_ruleset_version_id(),
        procedure_results,
        analysis_summary: summary,
        findings: all_findings,
        output_tables: all_output_tables,
        visual_outputs: all_visual_outputs,
        detail_refs: Vec::new(),
        warnings: all_warnings,
        errors: all_errors,
        fatal_error: None,
        sampling,
    };

    // --- 10. Create Execution Metadata ---
    let metadata = metadata_base();

    // --- 11. Submit Analysis Report Request / Return Final Execution
    // Status --- (F.1: the engine's job ends here; the Report Engine
    // alone constructs the Analysis Report from this output — see
    // `engines::report::generate_analysis_report`, never called from
    // within this engine, per INV-010/015.)
    (result, metadata, output_table_content)
}
