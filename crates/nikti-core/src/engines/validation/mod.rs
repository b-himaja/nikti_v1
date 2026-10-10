//! Validation Engine (Validation Engine Runtime Contract v1.0, as
//! summarized in the governing task prompt — the full source document
//! was unavailable in this repository at implementation time; see the
//! final compliance report). INV-012: executes Validation Rulesets
//! only — never infers business intent beyond what the bound Ruleset
//! explicitly states. INV-010: never calls another engine.

pub mod catalogue;
pub mod functions;
#[cfg(test)]
mod tests;

use polars::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::types::common::{ProjectId, Timestamp};
use crate::types::{RuleNullPolicy, Severity, ValidationRulesetVersion};

use super::content::{DatasetContent, TableContent};

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error("request error: {0}")]
    Request(String),
}

/// Amendment 10 (binding): closed outcome vocabulary. `warning` is never
/// a valid `outcome` — see `Severity` for that (a rule property, not a
/// result).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RuleOutcome {
    Passed,
    Failed,
    NotExecutable,
    Skipped,
    ExecutionError,
}

/// A reference, never embedded data (C.4) — uniform across every
/// function regardless of how small the failing-row set happens to be,
/// mirroring Inspection's sampling-disclosure discipline of never
/// special-casing "this one's small enough to just embed."
#[derive(Debug, Clone, Serialize)]
pub struct AffectedRowsRef {
    pub reference: String,
    pub failed_row_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleExecutionResult {
    pub rule_result_id: Uuid,
    pub rule_id: Uuid,
    pub rule_instance_id: Uuid,
    pub function_id: String,
    pub outcome: RuleOutcome,
    pub severity: Severity,
    pub evaluated_count: u64,
    pub passed_count: u64,
    pub failed_count: u64,
    pub affected_columns: Vec<String>,
    pub affected_rows_ref: Option<AffectedRowsRef>,
    pub message: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OverallStatus {
    Passed,
    Failed,
    CompletedWithErrors,
    Cancelled,
    FailedExecution,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationSummary {
    pub total_rules: u64,
    pub enabled_rules: u64,
    pub executed_rules: u64,
    pub passed_rules: u64,
    pub failed_rules: u64,
    pub not_executable_rules: u64,
    pub skipped_rules: u64,
    pub execution_error_rules: u64,
    pub critical_failures: u64,
    pub error_failures: u64,
    pub warning_failures: u64,
    pub information_failures: u64,
    pub overall_status: OverallStatus,
}

/// INV-012/014: rule results and computed measurements only — never
/// inferred business intent. Raw engine output; only
/// `ReportEngine::generate_validation_report` wraps this into a
/// `ValidationReport` (INV-015).
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResults {
    pub rule_results: Vec<RuleExecutionResult>,
    pub summary: ValidationSummary,
    /// C.8: a fatal `request_error` — set only when execution stopped
    /// before any rule ran. Distinct from any individual rule's
    /// `ExecutionError` outcome, which never aborts the run.
    pub request_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TargetObjectType {
    Table,
    Dataset,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationExecutionMetadata {
    pub execution_id: Uuid,
    pub target_object_type: TargetObjectType,
    pub target_object_id: Uuid,
    /// C.6: required if the target is a Dataset, `None` for a Table —
    /// structurally guaranteed by `ValidationTarget`'s shape, not just a
    /// runtime check (see below).
    pub project_id: Option<ProjectId>,
    pub target_fingerprint: Option<String>,
    pub ruleset_id: Uuid,
    pub ruleset_version_id: Uuid,
    pub ruleset_hash: String,
    pub function_catalogue_version: String,
    pub engine_version: String,
    pub started_at: Timestamp,
    pub completed_at: Timestamp,
    pub actor: String,
}

/// C.6/C.7: which log a run routes to is determined entirely by which
/// variant this is — Table -> Table Event Log, Dataset -> Project
/// Operation Log with a mandatory `project_id`. There is deliberately no
/// way to construct a Dataset-targeted run without a `project_id`.
pub enum ValidationTarget {
    Table(TableContent),
    Dataset { content: DatasetContent, project_id: ProjectId },
}

impl ValidationTarget {
    fn frame(&self) -> LazyFrame {
        match self {
            ValidationTarget::Table(c) => c.frame.clone(),
            ValidationTarget::Dataset { content, .. } => content.frame.clone(),
        }
    }

    fn object_id(&self) -> Uuid {
        match self {
            ValidationTarget::Table(c) => c.table_id,
            ValidationTarget::Dataset { content, .. } => content.dataset_id,
        }
    }

    fn object_type(&self) -> TargetObjectType {
        match self {
            ValidationTarget::Table(_) => TargetObjectType::Table,
            ValidationTarget::Dataset { .. } => TargetObjectType::Dataset,
        }
    }

    fn project_id(&self) -> Option<ProjectId> {
        match self {
            ValidationTarget::Table(_) => None,
            ValidationTarget::Dataset { project_id, .. } => Some(*project_id),
        }
    }
}

/// C.7: which log a completed run's Execution Metadata belongs to.
/// Routing itself (the actual append) is a command-layer responsibility
/// (INV-010/023) — this is the routing *decision*, derived purely from
/// which `ValidationTarget` variant was used, so it can never disagree
/// with `ValidationExecutionMetadata::target_object_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRoute {
    TableEventLog,
    ProjectOperationLog { project_id: ProjectId },
}

pub fn log_route_for(metadata: &ValidationExecutionMetadata) -> LogRoute {
    match metadata.target_object_type {
        TargetObjectType::Table => LogRoute::TableEventLog,
        TargetObjectType::Dataset => LogRoute::ProjectOperationLog {
            project_id: metadata.project_id.expect("C.6: project_id is mandatory for a Dataset target"),
        },
    }
}

fn resolve_policy(rule_policy: RuleNullPolicy, ruleset_default: RuleNullPolicy) -> RuleNullPolicy {
    match rule_policy {
        RuleNullPolicy::InheritRulesetDefault => ruleset_default,
        other => other,
    }
}

/// INV-010: executes Validation Rulesets only — never modifies data,
/// never calls Transformation or any other engine, never writes to a
/// Table or Dataset, never produces conclusions without a bound Ruleset
/// (INV-012).
pub trait ValidationEngine {
    /// `definition` is always a specific, already-resolved
    /// `ValidationRulesetVersion` — there is no `ValidationRuleset`
    /// overload and none may ever be added; "the runtime must never
    /// resolve or execute an unresolved latest" is enforced by this
    /// signature, not by a runtime check.
    fn validate(
        &self,
        target: ValidationTarget,
        ruleset_id: Uuid,
        definition: &ValidationRulesetVersion,
        actor: String,
        started_at: Timestamp,
    ) -> (ValidationResults, ValidationExecutionMetadata, Vec<(String, DataFrame)>) {
        let _ = (target, ruleset_id, definition, actor, started_at);
        todo!()
    }
}

pub struct NativeValidationEngine;

impl ValidationEngine for NativeValidationEngine {
    fn validate(
        &self,
        target: ValidationTarget,
        ruleset_id: Uuid,
        definition: &ValidationRulesetVersion,
        actor: String,
        started_at: Timestamp,
    ) -> (ValidationResults, ValidationExecutionMetadata, Vec<(String, DataFrame)>) {
        validate_standard(target, ruleset_id, definition, actor, started_at, "0.1.0", &|_| false)
    }
}

/// Internal, independently testable entry point. `cancel` is checked
/// before each rule; once it returns `true`, every remaining rule is
/// recorded `Skipped` (reason: "cancelled") and the run's
/// `overall_status` becomes `Cancelled` — never silently absent (C.9).
pub fn validate_standard(
    target: ValidationTarget,
    ruleset_id: Uuid,
    ruleset_version: &ValidationRulesetVersion,
    actor: String,
    started_at: Timestamp,
    engine_version: &str,
    cancel: &dyn Fn(usize) -> bool,
) -> (ValidationResults, ValidationExecutionMetadata, Vec<(String, DataFrame)>) {
    let execution_id = Uuid::new_v4();
    let metadata_base = |completed_at: Timestamp| ValidationExecutionMetadata {
        execution_id,
        target_object_type: target.object_type(),
        target_object_id: target.object_id(),
        project_id: target.project_id(),
        target_fingerprint: None,
        ruleset_id,
        ruleset_version_id: ruleset_version.ruleset_version_id(),
        ruleset_hash: ruleset_version.ruleset_hash().to_string(),
        function_catalogue_version: ruleset_version.function_catalogue_version().to_string(),
        engine_version: engine_version.to_string(),
        started_at,
        completed_at,
        actor: actor.clone(),
    };

    // --- 1. Validate Request ---
    // Compatibility is a request-level concern here: if this
    // ValidationRulesetVersion doesn't support this target's object type
    // at all, nothing is executable — that's a `request_error`, not 30
    // individual not_executable rules.
    let target_kind_compatible = ruleset_version.compatible_target_types().iter().any(|t| match (t, target.object_type()) {
        (crate::types::CompatibleTargetObjectType::Table, TargetObjectType::Table) => true,
        (crate::types::CompatibleTargetObjectType::Dataset, TargetObjectType::Dataset) => true,
        _ => false,
    });
    if !target_kind_compatible {
        let results = ValidationResults {
            rule_results: Vec::new(),
            summary: empty_summary(OverallStatus::FailedExecution),
            request_error: Some("ruleset version is not compatible with this target's object type".to_string()),
        };
        return (results, metadata_base(started_at), Vec::new());
    }

    // --- 2. Load Read-Only Target ---
    let df = match target.frame().collect() {
        Ok(df) => df,
        Err(e) => {
            let results = ValidationResults {
                rule_results: Vec::new(),
                summary: empty_summary(OverallStatus::FailedExecution),
                request_error: Some(format!("target could not be loaded: {e}")),
            };
            return (results, metadata_base(started_at), Vec::new());
        }
    };
    let schema_columns: std::collections::HashSet<String> = df.get_column_names().into_iter().map(|s| s.to_string()).collect();

    // --- 3. Load Ruleset Version --- (already the `ruleset_version` param)
    // --- 4. Resolve Bindings / --- 5. Resolve References ---
    // (References are supplied pre-resolved as literal value lists on
    // the rule's own parameters in this implementation — see
    // `ValidationFunctionParams`'s doc comment; there is no
    // repository-backed reference-object loader in this codebase yet.)

    let mut rule_results = Vec::new();
    // The real content behind every `AffectedRowsRef` above — the command
    // layer writes these to real storage after this function returns.
    let mut affected_rows_content: Vec<(String, DataFrame)> = Vec::new();
    for (idx, rule) in ruleset_version.rules().iter().enumerate() {
        let rule_result_id = Uuid::new_v4();
        let base = |outcome: RuleOutcome, message: String| RuleExecutionResult {
            rule_result_id,
            rule_id: rule.rule_id(),
            rule_instance_id: rule.rule_instance_id(),
            function_id: rule.function_id().to_string(),
            outcome,
            severity: rule.severity(),
            evaluated_count: 0,
            passed_count: 0,
            failed_count: 0,
            affected_columns: rule.target().target_columns.clone(),
            affected_rows_ref: None,
            message,
            diagnostics: Vec::new(),
        };

        if !rule.enabled() {
            rule_results.push(base(RuleOutcome::Skipped, "rule is disabled".to_string()));
            continue;
        }

        // C.9: cancellation checked before each rule; the run doesn't
        // stop mid-rule, it simply never starts the next one.
        if cancel(idx) {
            rule_results.push(base(RuleOutcome::Skipped, "cancelled".to_string()));
            continue;
        }

        // Amendment 3 (catalogue rejection): unsupported function_id.
        if !catalogue::is_supported(rule.function_id()) {
            rule_results.push(base(RuleOutcome::NotExecutable, format!("function_id '{}' is not in the Function Catalogue", rule.function_id())));
            continue;
        }

        // Resolve Bindings (A.3/A.4): missing target column -> this rule
        // only becomes not_executable, never the whole run.
        let missing_columns: Vec<&String> = rule.target().target_columns.iter().filter(|c| !schema_columns.contains(*c)).collect();
        if !missing_columns.is_empty() {
            rule_results.push(base(
                RuleOutcome::NotExecutable,
                format!("target column(s) not present: {}", missing_columns.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
            ));
            continue;
        }

        let policy = resolve_policy(rule.null_policy(), ruleset_version.missing_value_policy().default_null_policy);

        // C.8: a single rule's technical failure never aborts the run —
        // caught here and recorded as `ExecutionError`, execution
        // continues to the next rule.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            functions::execute_function(&df, rule.function(), rule.target(), policy)
        })) {
            Ok(Ok(outcome)) => {
                let rule_outcome = if outcome.failed_count > 0 { RuleOutcome::Failed } else { RuleOutcome::Passed };
                // Only ever set when real failing-row content exists
                // (`outcome.affected_rows`) — never a reference to
                // nothing, and never set at all for the two aggregate
                // functions with no natural per-row subset.
                let affected_rows_ref = match (outcome.failed_count, outcome.affected_rows) {
                    (count, Some(rows)) if count > 0 => {
                        let reference = format!("validation-failed-rows/{rule_result_id}.parquet");
                        affected_rows_content.push((reference.clone(), rows));
                        Some(AffectedRowsRef { reference, failed_row_count: count })
                    }
                    _ => None,
                };
                rule_results.push(RuleExecutionResult {
                    rule_result_id,
                    rule_id: rule.rule_id(),
                    rule_instance_id: rule.rule_instance_id(),
                    function_id: rule.function_id().to_string(),
                    outcome: rule_outcome,
                    severity: rule.severity(),
                    evaluated_count: outcome.evaluated_count,
                    passed_count: outcome.passed_count,
                    failed_count: outcome.failed_count,
                    affected_columns: outcome.affected_columns,
                    affected_rows_ref,
                    message: rule.failure_message_template().to_string(),
                    diagnostics: Vec::new(),
                });
            }
            Ok(Err(e)) => {
                rule_results.push(base(RuleOutcome::ExecutionError, format!("execution error: {e}")));
            }
            Err(_) => {
                rule_results.push(base(RuleOutcome::ExecutionError, "execution error: rule evaluation panicked".to_string()));
            }
        }
    }

    let cancelled = rule_results.iter().any(|r| r.outcome == RuleOutcome::Skipped && r.message == "cancelled");
    let enabled_rules = ruleset_version.rules().iter().filter(|r| r.enabled()).count() as u64;
    let summary = summarize(ruleset_version.rules().len() as u64, enabled_rules, &rule_results, cancelled);

    let results = ValidationResults { rule_results, summary, request_error: None };
    let mut metadata = metadata_base(started_at);
    metadata.completed_at = started_at;
    (results, metadata, affected_rows_content)
}

fn empty_summary(overall_status: OverallStatus) -> ValidationSummary {
    ValidationSummary {
        total_rules: 0,
        enabled_rules: 0,
        executed_rules: 0,
        passed_rules: 0,
        failed_rules: 0,
        not_executable_rules: 0,
        skipped_rules: 0,
        execution_error_rules: 0,
        critical_failures: 0,
        error_failures: 0,
        warning_failures: 0,
        information_failures: 0,
        overall_status,
    }
}

fn summarize(total_rules: u64, enabled_rules: u64, results: &[RuleExecutionResult], cancelled: bool) -> ValidationSummary {
    let mut s = empty_summary(OverallStatus::Passed);
    s.total_rules = total_rules;
    s.enabled_rules = enabled_rules;
    for r in results {
        match r.outcome {
            RuleOutcome::Passed => {
                s.executed_rules += 1;
                s.passed_rules += 1;
            }
            RuleOutcome::Failed => {
                s.executed_rules += 1;
                s.failed_rules += 1;
                match r.severity {
                    Severity::Critical => s.critical_failures += 1,
                    Severity::Error => s.error_failures += 1,
                    Severity::Warning => s.warning_failures += 1,
                    Severity::Information => s.information_failures += 1,
                }
            }
            RuleOutcome::NotExecutable => s.not_executable_rules += 1,
            RuleOutcome::Skipped => s.skipped_rules += 1,
            RuleOutcome::ExecutionError => s.execution_error_rules += 1,
        }
    }

    s.overall_status = if cancelled {
        OverallStatus::Cancelled
    } else if s.execution_error_rules > 0 {
        OverallStatus::CompletedWithErrors
    } else if s.failed_rules > 0 {
        OverallStatus::Failed
    } else {
        OverallStatus::Passed
    };
    s
}
