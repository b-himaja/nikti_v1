//! Report Engine. INV-015/029: the one place anywhere permitted to
//! construct a Report — no workspace, repository, engine, or the Python
//! sidecar ever constructs one independently (Part 5 item 5).
//!
//! As of the Audit Report task, all six `generate_*_report` methods
//! (Inspection, Validation, Analysis, Lineage, Comparison, Audit) are
//! real, in `impl ReportEngine for NativeReportEngine` below — none of
//! them are `todo!()` stubs. This comment previously said only
//! Inspection was implemented; that was accurate when it was written
//! (this task's original scope) and became stale as later tasks filled
//! in the rest, with nothing catching the drift until status audit 3.
//! The trait itself still declares `todo!()` *default* methods (see
//! `ReportEngine`'s own definition) purely so a hypothetical future
//! engine implementation isn't forced to implement all six at once —
//! that's a deliberate trait-design affordance, not a sign any of
//! `NativeReportEngine`'s own six methods are unfinished.

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::repositories::AnyReport;
use crate::types::common::{EngineVersion, ReportId, ReportKind, TableId, Timestamp};
use crate::types::report_registration::{SourceContext, SourceContextKind, SourceEventRef};
use crate::types::{
    structural_diff, AnalysisReport, AnalysisRulesetVersion, AuditContent, AuditReport, ComparisonContent, ComparisonOutcome, ComparisonReport, InspectionReport, LineageContent, LineageReport,
    ReportState, ValidationReport, ValidationRulesetVersion,
};

use super::analysis::{AnalysisExecutionMetadata, AnalysisResult, ProcedureStatus};
use super::inspection::InspectionResults;
use super::validation::{RuleOutcome, ValidationExecutionMetadata, ValidationResults};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportEngineError {
    /// INV-016: Inspection<->Inspection, Validation<->Validation,
    /// Analysis<->Analysis, Lineage<->Lineage, Comparison<->Comparison,
    /// Audit<->Audit. Enforced in
    /// `NativeReportEngine::generate_comparison_report` itself, the one
    /// canonical choke point every comparison passes through — not just
    /// documented, not just checked upstream.
    #[error("cannot compare reports of different kinds")]
    MismatchedReportKinds,
    /// Reserved for a report kind that exists in the `AnyReport` enum
    /// but has no real producer — not currently reachable, since all six
    /// kinds are real as of the Audit Report task (`register_audit_report`
    /// replaced the old `insert_audit` stub). Kept distinct from
    /// `MismatchedReportKinds` in case a future kind is added to the
    /// enum before its own generator lands.
    #[error("this report kind is not yet supported for comparison")]
    UnsupportedReportKind,
}

/// The Report Engine's own output for an Inspection Report: the Report
/// Object (metadata/pointer only, per the Report Object Model — see
/// `InspectionReport`'s doc comment on why content isn't embedded there)
/// paired with the actual content it points to. The command layer
/// serializes `content` and hands both to
/// `ReportRepository::register_inspection_report` (B.3).
#[derive(Debug, Clone)]
pub struct GeneratedInspectionReport {
    pub report: InspectionReport,
    pub content: InspectionResults,
}

/// Same shape as `GeneratedInspectionReport`, for Lineage: the command
/// layer builds `LineageContent` by querying already-real references
/// (Rule 8/INV-010: this is not a processing engine computing something
/// new), and hands it to `generate_lineage_report` below, which alone
/// wraps it into the evidence object (INV-015).
#[derive(Debug, Clone)]
pub struct GeneratedLineageReport {
    pub report: LineageReport,
    pub content: LineageContent,
}

/// Same shape again, for Audit: the command layer (`project_workspace::
/// run_transformation_steps`) has already assembled real `AuditContent`
/// from the transformation it just ran — this function alone wraps it
/// into evidence (INV-015). Unlike Lineage/Comparison, this genuinely
/// is triggered by a real engine execution, not a query — see
/// `source_event_ref.source_event_type` at the call site below.
#[derive(Debug, Clone)]
pub struct GeneratedAuditReport {
    pub report: AuditReport,
    pub content: AuditContent,
}

/// Same shape again, for Comparison: the command layer fetches both
/// input reports' real metadata and real content bytes and hands them
/// here — this function alone performs the type/schema checks and the
/// structural diff, and alone wraps the result into evidence.
#[derive(Debug, Clone)]
pub struct GeneratedComparisonReport {
    pub report: ComparisonReport,
    pub content: ComparisonContent,
}

fn any_report_kind(report: &AnyReport) -> ReportKind {
    match report {
        AnyReport::Inspection(r) => r.report_type(),
        AnyReport::Validation(r) => r.report_type(),
        AnyReport::Analysis(r) => r.report_type(),
        AnyReport::Lineage(r) => r.report_type(),
        AnyReport::Comparison(r) => r.report_type(),
        AnyReport::Audit(r) => r.report_type(),
    }
}

/// Real for all six `AnyReport` kinds as of the Audit Report task —
/// `AuditReport` moved onto the same baseline shape every other kind
/// uses, so there's no longer a kind with no `report_schema_version` at
/// all. `Option` kept for the same forward-compatible "resolve or skip"
/// shape as `any_report_identity` in `commands.rs`.
fn any_report_id_and_schema_version(report: &AnyReport) -> Option<(ReportId, String)> {
    match report {
        AnyReport::Inspection(r) => Some((r.report_id(), r.report_schema_version().to_string())),
        AnyReport::Validation(r) => Some((r.report_id(), r.report_schema_version().to_string())),
        AnyReport::Analysis(r) => Some((r.report_id(), r.report_schema_version().to_string())),
        AnyReport::Lineage(r) => Some((r.report_id(), r.report_schema_version().to_string())),
        AnyReport::Comparison(r) => Some((r.report_id(), r.report_schema_version().to_string())),
        AnyReport::Audit(r) => Some((r.report_id(), r.report_schema_version().to_string())),
    }
}

fn content_hash_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Part D: fixed content-section order — binding, not a suggestion.
pub const VALIDATION_REPORT_SECTION_ORDER: &[&str] = &[
    "Report Header",
    "Execution Summary",
    "Rule Summary",
    "Severity Summary",
    "Validation Findings",
    "Not Executed/Skipped Rules",
    "Corrective Notes",
    "Execution Metadata",
    "Appendix",
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationReportSection {
    pub name: String,
    pub body: String,
}

/// The actual materialized report content, addressed by
/// `ValidationReport::content_ref` once registered — see
/// `InspectionReport`'s doc comment for why content isn't embedded in
/// the canonical Report object itself.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationReportContent {
    pub sections: Vec<ValidationReportSection>,
    pub results: ValidationResults,
    pub execution_metadata: ValidationExecutionMetadata,
}

#[derive(Debug, Clone)]
pub struct GeneratedValidationReport {
    pub report: ValidationReport,
    pub content: ValidationReportContent,
}

fn build_validation_sections(
    results: &ValidationResults,
    metadata: &ValidationExecutionMetadata,
    ruleset_version: &ValidationRulesetVersion,
) -> Vec<ValidationReportSection> {
    let section = |name: &str, body: String| ValidationReportSection { name: name.to_string(), body };
    let s = &results.summary;

    let findings = results
        .rule_results
        .iter()
        .filter(|r| r.outcome == RuleOutcome::Failed)
        .map(|r| format!("[{:?}] {} on {}: {}", r.severity, r.function_id, r.affected_columns.join(","), r.message))
        .collect::<Vec<_>>()
        .join("\n");

    let not_executed = results
        .rule_results
        .iter()
        .filter(|r| matches!(r.outcome, RuleOutcome::NotExecutable | RuleOutcome::Skipped | RuleOutcome::ExecutionError))
        .map(|r| format!("[{:?}] {}: {}", r.outcome, r.function_id, r.message))
        .collect::<Vec<_>>()
        .join("\n");

    // Amendment 9: rule-authored messages ONLY — each is exactly the
    // Ruleset author's own `failure_message_template`
    // (`RuleExecutionResult::message` is set verbatim from it in the
    // engine, never engine-invented text — see `validation::mod`).
    let corrective_notes = results
        .rule_results
        .iter()
        .filter(|r| r.outcome == RuleOutcome::Failed)
        .map(|r| r.message.clone())
        .collect::<Vec<_>>()
        .join("\n");

    vec![
        section("Report Header", format!("Validation Report — ruleset_version {}", ruleset_version.ruleset_version_id())),
        section("Execution Summary", format!("overall_status={:?} total_rules={} executed_rules={}", s.overall_status, s.total_rules, s.executed_rules)),
        section(
            "Rule Summary",
            format!(
                "enabled={} passed={} failed={} not_executable={} skipped={} execution_error={}",
                s.enabled_rules, s.passed_rules, s.failed_rules, s.not_executable_rules, s.skipped_rules, s.execution_error_rules
            ),
        ),
        section(
            "Severity Summary",
            format!("critical={} error={} warning={} information={}", s.critical_failures, s.error_failures, s.warning_failures, s.information_failures),
        ),
        section("Validation Findings", findings),
        section("Not Executed/Skipped Rules", not_executed),
        section("Corrective Notes", corrective_notes),
        section("Execution Metadata", format!("{metadata:?}")),
        section("Appendix", String::new()),
    ]
}

/// F.2: fixed section order. Header/Execution Summary/Target & Scope/
/// Procedure Summary/Analysis Summary/Procedure Results/Execution
/// Metadata Summary/Evidence Trail are unconditional; the remaining four
/// are each included only when their condition holds (never as an
/// always-present-but-empty placeholder) — see `build_analysis_sections`.
pub const ANALYSIS_REPORT_SECTION_ORDER_TEMPLATE: &[&str] = &[
    "Header",
    "Execution Summary",
    "Target & Scope",
    "Procedure Summary",
    "Analysis Summary",
    "Findings",
    "Procedure Results",
    "Feature & Row Attention Evidence",
    "Visual Evidence",
    "Warnings/Not Executed",
    "Execution Metadata Summary",
    "Evidence Trail",
    "Appendix",
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisReportSection {
    pub name: String,
    pub body: String,
}

/// The actual materialized report content, addressed by
/// `AnalysisReport::content_ref` once registered.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisReportContent {
    pub sections: Vec<AnalysisReportSection>,
    pub result: AnalysisResult,
    pub execution_metadata: AnalysisExecutionMetadata,
}

#[derive(Debug, Clone)]
pub struct GeneratedAnalysisReport {
    pub report: AnalysisReport,
    pub content: AnalysisReportContent,
}

fn build_analysis_sections(result: &AnalysisResult, metadata: &AnalysisExecutionMetadata, ruleset_version: &AnalysisRulesetVersion) -> Vec<AnalysisReportSection> {
    let section = |name: &str, body: String| AnalysisReportSection { name: name.to_string(), body };
    let s = &result.analysis_summary;
    let mut sections = Vec::new();

    sections.push(section("Header", format!("Analysis Report — ruleset_version {}", ruleset_version.analysis_ruleset_version_id())));
    sections.push(section("Execution Summary", format!("execution_status={:?} total_procedures={}", result.execution_status, s.total_procedures)));
    sections.push(section(
        "Target & Scope",
        format!("target_object_type={:?} target_object_id={} sampling={:?}", metadata.target_object_type, metadata.target_object_id, result.sampling),
    ));
    sections.push(section(
        "Procedure Summary",
        format!(
            "completed={} completed_with_warnings={} skipped={} not_executable={} execution_error={}",
            s.completed_procedures, s.completed_with_warnings, s.skipped_procedures, s.not_executable_procedures, s.execution_error_procedures
        ),
    ));
    sections.push(section(
        "Analysis Summary",
        format!("total_output_tables={} total_visual_outputs={} total_findings={} total_warnings={}", s.total_output_tables, s.total_visual_outputs, s.total_findings, s.total_warnings),
    ));

    // Findings — conditional: only when at least one exists.
    if !result.findings.is_empty() {
        let body = result.findings.iter().map(|f| format!("[{:?}] {}", f.finding_type, f.statement)).collect::<Vec<_>>().join("\n");
        sections.push(section("Findings", body));
    }

    // Procedure Results — one per enabled procedure, unconditional
    // (always at least AP-001).
    let body = result
        .procedure_results
        .iter()
        .map(|r| format!("[{:?}] {} ({}): {} measure(s), {} finding(s)", r.status, r.procedure_name, r.procedure_type, r.measures.len(), r.findings.len()))
        .collect::<Vec<_>>()
        .join("\n");
    sections.push(section("Procedure Results", body));

    // Feature & Row Attention Evidence — conditional: only if AP-019 ran.
    let ap019 = result.procedure_results.iter().find(|r| r.procedure_type == "AP-019");
    if let Some(ap019) = ap019 {
        let body = ap019.findings.iter().map(|f| format!("[{:?}] {}", f.finding_type, f.statement)).collect::<Vec<_>>().join("\n");
        sections.push(section("Feature & Row Attention Evidence", body));
    }

    // Visual Evidence — conditional: only when at least one visual output exists.
    if !result.visual_outputs.is_empty() {
        let body = result.visual_outputs.iter().map(|v| format!("[{:?}] {}", v.visual_type, v.reference)).collect::<Vec<_>>().join("\n");
        sections.push(section("Visual Evidence", body));
    }

    // Warnings/Not Executed — conditional.
    let not_executed: Vec<&super::analysis::ProcedureResult> =
        result.procedure_results.iter().filter(|r| matches!(r.status, ProcedureStatus::Skipped | ProcedureStatus::NotExecutable | ProcedureStatus::ExecutionError)).collect();
    if !result.warnings.is_empty() || !not_executed.is_empty() {
        let mut body = result.warnings.join("\n");
        for r in not_executed {
            body.push_str(&format!("\n[{:?}] {}: {}", r.status, r.procedure_name, r.errors.join("; ")));
        }
        sections.push(section("Warnings/Not Executed", body));
    }

    sections.push(section("Execution Metadata Summary", format!("{metadata:?}")));
    sections.push(section("Evidence Trail", format!("ruleset_hash={} catalogue_version={}", metadata.ruleset_hash, metadata.analysis_procedure_catalogue_version)));

    // Appendix — conditional: only when there are detail refs to list.
    if !result.detail_refs.is_empty() {
        sections.push(section("Appendix", result.detail_refs.join("\n")));
    }

    sections
}

/// INV-015/029: the one function anywhere permitted to construct a
/// Report. Never writes to the Table it describes (B.5). Never overwrites
/// an existing Report — every call produces a new `report_id`.
pub trait ReportEngine {
    fn generate_inspection_report(
        &self,
        results: InspectionResults,
        source_table_id: crate::types::common::TableId,
        source_execution_id: Uuid,
        engine_version: EngineVersion,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedInspectionReport {
        let _ = (results, source_table_id, source_execution_id, engine_version, created_by, created_at);
        todo!()
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_validation_report(
        &self,
        results: ValidationResults,
        execution_metadata: ValidationExecutionMetadata,
        ruleset_version: &ValidationRulesetVersion,
        source_context: SourceContext,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedValidationReport {
        let _ = (results, execution_metadata, ruleset_version, source_context, created_by, created_at);
        todo!()
    }

    /// F.1 (Amendment 9, binding): the Analysis Engine only ever emits a
    /// Report Request (its output — `AnalysisResult` +
    /// `AnalysisExecutionMetadata` — handed to this function by the
    /// command layer, never by the Analysis Engine calling this itself,
    /// INV-010). This function alone constructs the Analysis Report.
    #[allow(clippy::too_many_arguments)]
    fn generate_analysis_report(
        &self,
        result: AnalysisResult,
        execution_metadata: AnalysisExecutionMetadata,
        ruleset_version: &AnalysisRulesetVersion,
        source_context: SourceContext,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedAnalysisReport {
        let _ = (result, execution_metadata, ruleset_version, source_context, created_by, created_at);
        todo!()
    }

    /// Part C: `content` is already fully assembled real data by the
    /// time this is called — every field in `AuditContent` traces to a
    /// value `run_transformation_steps` computed from the transformation
    /// it just actually ran, never recomputed or estimated here. This
    /// function's only job is the same as every other `generate_*_report`
    /// method: hash the content, mint a report id, wrap it as evidence.
    fn generate_audit_report(&self, content: AuditContent, project_id: crate::types::common::ProjectId, source_execution_id: Uuid, source_engine: String, created_by: String, created_at: Timestamp) -> GeneratedAuditReport {
        let _ = (content, project_id, source_execution_id, source_engine, created_by, created_at);
        todo!()
    }

    /// Lineage Report task: the command layer has already resolved
    /// `content` (real nodes/edges, queried from already-tracked
    /// references — see `commands::generate_lineage_report`'s own doc
    /// comment) — this function's only job is wrapping it into the
    /// evidence object (INV-015), same division of labor as every other
    /// `generate_*_report` here.
    fn generate_lineage_report(
        &self,
        content: LineageContent,
        source_context: SourceContext,
        source_execution_id: Uuid,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedLineageReport {
        let _ = (content, source_context, source_execution_id, created_by, created_at);
        todo!()
    }

    /// INV-016: same-kind only; the one function that would ever combine
    /// two existing Reports into a Comparison view must reject mismatched
    /// `report_type`s rather than construct one — not built out in this
    /// task (no second report kind is implemented yet to compare against).
    /// Part B/C: `content_a`/`content_b` are the two reports' real,
    /// already-fetched content bodies — the command layer resolves
    /// these via `ReportRepository::get_content`, mirroring exactly how
    /// the Report Viewer resolves content for display (same honest,
    /// generic-JSON approach, not a second mechanism).
    #[allow(clippy::too_many_arguments)]
    fn generate_comparison_report(
        &self,
        report_a: &AnyReport,
        report_b: &AnyReport,
        content_a: serde_json::Value,
        content_b: serde_json::Value,
        source_execution_id: Uuid,
        created_by: String,
        created_at: Timestamp,
    ) -> Result<GeneratedComparisonReport, ReportEngineError> {
        let _ = (report_a, report_b, content_a, content_b, source_execution_id, created_by, created_at);
        todo!()
    }
}

pub struct NativeReportEngine;

impl ReportEngine for NativeReportEngine {
    fn generate_inspection_report(
        &self,
        results: InspectionResults,
        source_table_id: crate::types::common::TableId,
        source_execution_id: Uuid,
        engine_version: EngineVersion,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedInspectionReport {
        let content_bytes = serde_json::to_vec(&results).expect("InspectionResults is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();

        // Content-addressed: the ref is derived from the hash, not chosen
        // by the caller — matches the frozen "content-addressed Parquet"
        // storage decision even though the on-disk format here is JSON
        // pending real Parquet-backed content storage.
        let content_ref = format!("inspection-report-content/{content_hash}.json");

        let source_context = SourceContext { source_context_type: SourceContextKind::Table, table_id: Some(source_table_id), project_dataset_id: None, project_id: None };
        let source_event_ref = SourceEventRef {
            source_event_type: "table_inspection".to_string(),
            source_execution_id,
            source_engine: "inspection_engine".to_string(),
            source_engine_version: engine_version,
            source_execution_status: "completed".to_string(),
        };

        // Constructed at `Registered` — the repository is what actually
        // decides `Available` vs `Unavailable`/stays-`Registered` during
        // registration (B.3); the Report Engine never assigns
        // `Available` itself.
        let report = InspectionReport::new(
            report_id,
            format!("Inspection Report — {source_table_id}"),
            ReportState::Registered,
            "1.0".to_string(),
            created_at,
            created_by,
            source_context,
            source_event_ref,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            Vec::new(), // Inspection has no ruleset, so no knowledge_refs.
            Vec::new(), // log_refs populated by the command layer, which
                        // has the Table Event Log entry id this task's
                        // orchestration doesn't have in isolation here.
        );

        GeneratedInspectionReport { report, content: results }
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_validation_report(
        &self,
        results: ValidationResults,
        execution_metadata: ValidationExecutionMetadata,
        ruleset_version: &ValidationRulesetVersion,
        source_context: SourceContext,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedValidationReport {
        let sections = build_validation_sections(&results, &execution_metadata, ruleset_version);
        let execution_id = execution_metadata.execution_id;
        let content = ValidationReportContent { sections, results, execution_metadata };

        let content_bytes = serde_json::to_vec(&content).expect("ValidationReportContent is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();
        let content_ref = format!("validation-report-content/{content_hash}.json");

        let source_label = source_context.table_id.map(|id| id.to_string()).or(source_context.project_dataset_id.map(|id| id.to_string())).unwrap_or_default();
        let source_event_ref = SourceEventRef {
            source_event_type: "table_validation".to_string(),
            source_execution_id: execution_id,
            source_engine: "validation_engine".to_string(),
            source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            source_execution_status: "completed".to_string(),
        };

        // Amendment: knowledge_refs carries exactly one entry — the
        // specific ruleset *version* used, never a bare ruleset id.
        let knowledge_refs = vec![ruleset_version.ruleset_version_id()];

        let report = ValidationReport::new(
            report_id,
            format!("Validation Report — {source_label}"),
            ReportState::Registered,
            "1.0".to_string(),
            "1.0".to_string(), // report_template_version: independent from report_schema_version (Part D).
            created_at,
            created_by,
            source_context,
            source_event_ref,
            Uuid::new_v4(), // source_validation_result_id: identifies this ValidationResults object.
            execution_id,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            knowledge_refs,
            Vec::new(), // log_refs populated by the command layer (C.7).
        );

        GeneratedValidationReport { report, content }
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_analysis_report(
        &self,
        result: AnalysisResult,
        execution_metadata: AnalysisExecutionMetadata,
        ruleset_version: &AnalysisRulesetVersion,
        source_context: SourceContext,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedAnalysisReport {
        let sections = build_analysis_sections(&result, &execution_metadata, ruleset_version);
        let source_execution_id = execution_metadata.execution_id;
        // F.5: composite engine_version — carried through verbatim from
        // the Analysis Engine's own execution metadata, which is already
        // `{ rust_core, python_env_hash }`-shaped (INV-030). No procedure
        // in the current frozen 19 is Python-backed (ADR 0007), so
        // `python_env_hash` is `None` in practice today — but the wiring
        // is real, not assumed: see
        // `report_validation_tests`-equivalent test
        // `composite_engine_version_flows_through_when_python_backed`.
        let engine_version = execution_metadata.engine_version.clone();
        let content = AnalysisReportContent { sections, result, execution_metadata };

        let content_bytes = serde_json::to_vec(&content).expect("AnalysisReportContent is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();
        let content_ref = format!("analysis-report-content/{content_hash}.json");

        let source_label = source_context.table_id.map(|id| id.to_string()).or(source_context.project_dataset_id.map(|id| id.to_string())).unwrap_or_default();
        let source_event_ref = SourceEventRef {
            source_event_type: "table_analysis".to_string(),
            source_execution_id,
            source_engine: "analysis_engine".to_string(),
            source_engine_version: engine_version,
            source_execution_status: "completed".to_string(),
        };

        let knowledge_refs = vec![ruleset_version.analysis_ruleset_version_id()];

        let report = AnalysisReport::new(
            report_id,
            format!("Analysis Report — {source_label}"),
            ReportState::Registered,
            "1.0".to_string(),
            created_at,
            created_by,
            source_context,
            source_event_ref,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            knowledge_refs,
            Vec::new(), // log_refs populated by the command layer (C.7).
        );

        GeneratedAnalysisReport { report, content }
    }

    fn generate_audit_report(&self, content: AuditContent, project_id: crate::types::common::ProjectId, source_execution_id: Uuid, source_engine: String, created_by: String, created_at: Timestamp) -> GeneratedAuditReport {
        let content_bytes = serde_json::to_vec(&content).expect("AuditContent is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();
        let content_ref = format!("audit-report-content/{content_hash}.json");

        // Part D: Audit Report ownership follows the Project — same
        // `project_dataset` source_context shape already used for
        // Dataset-target Validation/Analysis reports, reused rather than
        // inventing a new `source_context_type`.
        let source_context = SourceContext::for_project_dataset(project_id, content.output_dataset_id);

        // Part D: unlike Lineage/Comparison (`report_generation_event`),
        // this genuinely is produced by a real engine execution — the
        // Transformation Engine actually ran the steps this report
        // describes.
        let source_event_ref = SourceEventRef {
            source_event_type: "engine_execution".to_string(),
            source_execution_id,
            source_engine,
            source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            source_execution_status: "completed".to_string(),
        };

        let report = AuditReport::new(
            report_id,
            format!("Transformation Audit Report — {}", content.output_dataset_id),
            ReportState::Registered,
            "1.0".to_string(),
            created_at,
            created_by,
            source_context,
            source_event_ref,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            Vec::new(),
            content.operation_log_entry_ids.clone(),
        );

        GeneratedAuditReport { report, content }
    }

    fn generate_lineage_report(
        &self,
        content: LineageContent,
        source_context: SourceContext,
        source_execution_id: Uuid,
        created_by: String,
        created_at: Timestamp,
    ) -> GeneratedLineageReport {
        let content_bytes = serde_json::to_vec(&content).expect("LineageContent is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();
        let content_ref = format!("lineage-report-content/{content_hash}.json");

        let subject = source_context
            .table_id
            .map(|id| id.to_string())
            .or(source_context.project_dataset_id.map(|id| id.to_string()))
            .unwrap_or_default();

        // Part D: lineage is produced by the Report Engine directly
        // querying already-real data, never by a processing engine's own
        // execution — `source_event_type` reflects that distinction
        // rather than reusing the per-engine strings (`table_inspection`/
        // `table_validation`/`table_analysis`) every other report kind
        // uses. Note this isn't an enforced enum anywhere in this
        // codebase (`source_event_type` is a bare `String`) — this value
        // follows the task's own naming instruction as a reasonable,
        // honest free-text choice, not because the type system requires
        // it.
        let source_event_ref = SourceEventRef {
            source_event_type: "report_generation_event".to_string(),
            source_execution_id,
            source_engine: "report_engine".to_string(),
            source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            source_execution_status: "completed".to_string(),
        };

        let report = LineageReport::new(
            report_id,
            format!("Lineage Report — {subject}"),
            ReportState::Registered,
            "1.0".to_string(),
            created_at,
            created_by,
            source_context,
            source_event_ref,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            Vec::new(), // no ruleset/knowledge object involved in lineage.
            Vec::new(), // log_refs: lineage isn't itself logged as an
                        // operation-log entry anywhere it targets.
        );

        GeneratedLineageReport { report, content }
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_comparison_report(
        &self,
        report_a: &AnyReport,
        report_b: &AnyReport,
        content_a: serde_json::Value,
        content_b: serde_json::Value,
        source_execution_id: Uuid,
        created_by: String,
        created_at: Timestamp,
    ) -> Result<GeneratedComparisonReport, ReportEngineError> {
        // Part A: the one canonical choke point — a mismatched pair
        // never reaches diffing at all, let alone registration.
        let kind_a = any_report_kind(report_a);
        let kind_b = any_report_kind(report_b);
        if kind_a != kind_b {
            return Err(ReportEngineError::MismatchedReportKinds);
        }

        let (report_a_id, schema_a) = any_report_id_and_schema_version(report_a).ok_or(ReportEngineError::UnsupportedReportKind)?;
        let (report_b_id, schema_b) = any_report_id_and_schema_version(report_b).ok_or(ReportEngineError::UnsupportedReportKind)?;

        // Part C: a same-kind pair with incompatible schema versions is
        // a distinct, honest outcome — never silently diffed as if the
        // two content shapes lined up.
        let outcome = if schema_a != schema_b {
            ComparisonOutcome::SchemaVersionMismatch { report_a_schema_version: schema_a, report_b_schema_version: schema_b }
        } else {
            ComparisonOutcome::Diffed(structural_diff("", &content_a, &content_b))
        };

        let content = ComparisonContent { report_a_id, report_b_id, report_kind: kind_a, outcome };

        let content_bytes = serde_json::to_vec(&content).expect("ComparisonContent is always serializable");
        let content_hash = content_hash_of(&content_bytes);
        let report_id: ReportId = Uuid::new_v4();
        let content_ref = format!("comparison-report-content/{content_hash}.json");

        // `SourceContext` has no variant for "sourced from two other
        // Reports" (only Table/ProjectDataset/Project) — this is a
        // neutral placeholder, never treated as if it identified
        // something real. The two real inputs are carried on
        // `ComparisonReport` itself (`compared_report_a_id`/`_b_id`).
        let source_context = SourceContext { source_context_type: SourceContextKind::Project, table_id: None, project_dataset_id: None, project_id: None };
        let source_event_ref = SourceEventRef {
            source_event_type: "report_generation_event".to_string(),
            source_execution_id,
            source_engine: "report_engine".to_string(),
            source_engine_version: EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            source_execution_status: "completed".to_string(),
        };

        let report = ComparisonReport::new(
            report_id,
            format!("Comparison Report — {report_a_id} vs {report_b_id}"),
            ReportState::Registered,
            "1.0".to_string(),
            created_at,
            created_by,
            source_context,
            source_event_ref,
            content_ref,
            "application/json".to_string(),
            content_hash,
            None,
            Vec::new(),
            Vec::new(),
            report_a_id,
            report_b_id,
        );

        Ok(GeneratedComparisonReport { report, content })
    }
}
