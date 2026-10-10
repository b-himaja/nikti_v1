use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{DatasetId, LogEntryId, RecipeId, ReportId, ReportKind, Timestamp};
use super::report_registration::{ReportState, SourceContext, SourceEventRef};
use super::transformation_operation::TransformationOperation;

/// One real Transformation step within the run this report covers — the
/// same `TransformationOperation` the Project Operation Log entry for
/// this step carries (`ProjectOperationLogEntry::transformation_operation`),
/// paired with the real before/after counts already computed at the
/// point this step ran (`run_transformation`'s own loop) — never a
/// second, independently-recomputed measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditStep {
    pub operation: TransformationOperation,
    pub operation_log_entry_id: LogEntryId,
    pub row_count_before: u64,
    pub row_count_after: u64,
    pub column_count_before: usize,
    pub column_count_after: usize,
}

/// INV-001: every field here traces to a real value the Transformation
/// Engine/command layer already computed for this run — see
/// `commands::project_workspace::run_transformation_steps`'s own doc
/// comment for exactly where each one comes from.
///
/// `input_dataset_id`/`output_dataset_id` (and their `_name` pairs) are
/// always equal under the current data model: `run_transformation`
/// mutates a Dataset in place (`Dataset::apply_change`) rather than
/// producing a new `DatasetId` — the same structural fact the Lineage
/// Report task found and documented for its own `TransformationRun`
/// edge. Both fields are still carried, per this report's required
/// shape, rather than collapsed into one — but never implying they
/// could genuinely differ today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditContent {
    pub run_summary: String,
    pub input_dataset_id: DatasetId,
    pub input_dataset_name: String,
    pub output_dataset_id: DatasetId,
    pub output_dataset_name: String,
    pub recipe_id: Option<RecipeId>,
    pub recipe_version: Option<u32>,
    pub steps: Vec<AuditStep>,
    pub row_count_before: u64,
    pub row_count_after: u64,
    pub column_count_before: usize,
    pub column_count_after: usize,
    pub transformation_catalogue_version: String,
    pub input_schema_fingerprint: String,
    pub operation_log_entry_ids: Vec<LogEntryId>,
}

/// Category A evidence (Rule 5 / INV-004): no write method, ever. Same
/// baseline shape as every other report kind (Inspection/Validation/
/// Analysis/Lineage/Comparison) — not the old `ReportMeta` + bare
/// `{covered_entry_ids, summary_lines}` shape this type used before this
/// task, which had no `report_state`/`source_context`/`source_event_ref`
/// and so could never go through the same idempotent, state-machine-
/// gated registration flow every other report kind uses. Safe to change
/// field-for-field here: `insert_audit` was `todo!()` and no
/// `AuditReport` value has ever actually been constructed or persisted
/// anywhere in this codebase (confirmed directly, not assumed, before
/// this task started) — there is no existing evidence instance this
/// change could be mutating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    report_id: ReportId,
    report_type: ReportKind,
    report_title: String,
    report_state: ReportState,
    report_schema_version: String,
    created_at: Timestamp,
    created_by: String,
    generated_by_engine: String,
    source_context: SourceContext,
    source_event_ref: SourceEventRef,
    content_ref: String,
    content_format: String,
    content_hash: String,
    repository_registered_at: Option<Timestamp>,
    knowledge_refs: Vec<Uuid>,
    log_refs: Vec<LogEntryId>,
}

impl AuditReport {
    /// Intended to be called only from within the Report Engine
    /// (`generate_audit_report`, INV-015) and the Report Repository's
    /// registration algorithm (B.3).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        report_id: ReportId,
        report_title: String,
        report_state: ReportState,
        report_schema_version: String,
        created_at: Timestamp,
        created_by: String,
        source_context: SourceContext,
        source_event_ref: SourceEventRef,
        content_ref: String,
        content_format: String,
        content_hash: String,
        repository_registered_at: Option<Timestamp>,
        knowledge_refs: Vec<Uuid>,
        log_refs: Vec<LogEntryId>,
    ) -> Self {
        Self {
            report_id,
            report_type: ReportKind::Audit,
            report_title,
            report_state,
            report_schema_version,
            created_at,
            created_by,
            generated_by_engine: "report_engine".to_string(),
            source_context,
            source_event_ref,
            content_ref,
            content_format,
            content_hash,
            repository_registered_at,
            knowledge_refs,
            log_refs,
        }
    }

    pub fn report_id(&self) -> ReportId {
        self.report_id
    }

    pub fn report_type(&self) -> ReportKind {
        self.report_type
    }

    pub fn report_title(&self) -> &str {
        &self.report_title
    }

    pub fn report_state(&self) -> ReportState {
        self.report_state
    }

    pub fn report_schema_version(&self) -> &str {
        &self.report_schema_version
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }

    pub fn generated_by_engine(&self) -> &str {
        &self.generated_by_engine
    }

    pub fn source_context(&self) -> &SourceContext {
        &self.source_context
    }

    pub fn source_event_ref(&self) -> &SourceEventRef {
        &self.source_event_ref
    }

    pub fn content_ref(&self) -> &str {
        &self.content_ref
    }

    pub fn content_format(&self) -> &str {
        &self.content_format
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub fn repository_registered_at(&self) -> Option<Timestamp> {
        self.repository_registered_at
    }

    pub fn knowledge_refs(&self) -> &[Uuid] {
        &self.knowledge_refs
    }

    pub fn log_refs(&self) -> &[LogEntryId] {
        &self.log_refs
    }

    /// Not a mutation of `self` — see `InspectionReport::with_state`.
    pub fn with_state(&self, report_state: ReportState, repository_registered_at: Option<Timestamp>) -> Self {
        Self { report_state, repository_registered_at, ..self.clone() }
    }
}
