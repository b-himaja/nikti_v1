//! Report Object Model v1.0 / Report Repository Integration Contract v1.0
//! shared vocabulary. `source_context` and `source_event_ref` are always
//! nested objects on the canonical Report — never flattened onto the
//! Report's own top-level fields (a flattened view belongs only to a
//! future list/search index, not this object).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{DatasetId, EngineVersion, ProjectId, ReportId, TableId, Timestamp};

/// INV-004 Category A once a Report reaches `Available`: the only
/// transitions a Report ever makes are `Registered -> Available` and
/// `Registered -> Unavailable`, both performed by the repository
/// constructing a new `InspectionReport` value and replacing which value
/// is associated with a `report_id` in its own collection — never by
/// mutating a value already handed to a caller (see
/// `InspectionReport`'s doc comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportState {
    Registered,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceContextKind {
    Table,
    ProjectDataset,
    Project,
}

/// Nested per B.1 — never flatten these fields onto the Report itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceContext {
    pub source_context_type: SourceContextKind,
    pub table_id: Option<TableId>,
    pub project_dataset_id: Option<DatasetId>,
    pub project_id: Option<ProjectId>,
}

impl SourceContext {
    pub fn for_table(table_id: TableId) -> Self {
        Self {
            source_context_type: SourceContextKind::Table,
            table_id: Some(table_id),
            project_dataset_id: None,
            project_id: None,
        }
    }

    /// `commands::project_workspace::run_validation_on_dataset`/
    /// `run_analysis_on_dataset` — the Dataset-target counterpart to
    /// `for_table`. Did not exist before this task: `generate_validation_report`/
    /// `generate_analysis_report` previously hardcoded `SourceContextKind::Table`
    /// unconditionally, even though `SourceContextKind::ProjectDataset` was
    /// already a defined variant — a real gap this task closes, not a new
    /// invariant.
    pub fn for_project_dataset(project_id: ProjectId, dataset_id: DatasetId) -> Self {
        Self {
            source_context_type: SourceContextKind::ProjectDataset,
            table_id: None,
            project_dataset_id: Some(dataset_id),
            project_id: Some(project_id),
        }
    }
}

/// Nested per B.1. `source_engine` identifies who produced the *evidence*
/// being reported (e.g. `"inspection_engine"`) — distinct from the
/// Report's own `generated_by_engine`, which is always `"report_engine"`
/// (the thing that built the Report object). Never conflate the two.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEventRef {
    pub source_event_type: String,
    pub source_execution_id: Uuid,
    pub source_engine: String,
    pub source_engine_version: EngineVersion,
    pub source_execution_status: String,
}

/// B.3 step 2: everything the Report Object requires, plus the two
/// registration-specific identifiers. The engine computes `content_hash`
/// and constructs `idempotency_key` before submitting this.
#[derive(Debug, Clone)]
pub struct ReportRegistrationRequest {
    pub report_registration_request_id: Uuid,
    pub idempotency_key: String,
    pub report_id: ReportId,
    pub report_title: String,
    pub report_schema_version: String,
    pub created_at: Timestamp,
    pub created_by: String,
    pub source_context: SourceContext,
    pub source_event_ref: SourceEventRef,
    pub content_format: String,
    pub content_hash: String,
    pub content_bytes: Vec<u8>,
    pub knowledge_refs: Vec<Uuid>,
    pub log_refs: Vec<super::common::LogEntryId>,
}

/// Validation's variant of `ReportRegistrationRequest` — same baseline
/// fields plus the Part D additions (`report_template_version`,
/// `source_validation_result_id`, `execution_id`). Kept as its own
/// struct rather than folding Validation-specific fields into the
/// shared one, since Inspection Reports have no template-version/
/// execution-id concept at all and B.1's schema is not meant to grow
/// report-kind-specific optional fields.
#[derive(Debug, Clone)]
pub struct ValidationReportRegistrationRequest {
    pub report_registration_request_id: Uuid,
    pub idempotency_key: String,
    pub report_id: ReportId,
    pub report_title: String,
    pub report_schema_version: String,
    pub report_template_version: String,
    pub created_at: Timestamp,
    pub created_by: String,
    pub source_context: SourceContext,
    pub source_event_ref: SourceEventRef,
    pub source_validation_result_id: Uuid,
    pub execution_id: Uuid,
    pub content_format: String,
    pub content_hash: String,
    pub content_bytes: Vec<u8>,
    pub knowledge_refs: Vec<Uuid>,
    pub log_refs: Vec<super::common::LogEntryId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrationOutcome {
    /// A genuinely new Report was created and reached `Available`.
    Registered { report_id: ReportId },
    /// A genuinely new Report was created but content/index validation
    /// failed after step 5 — it stays `Registered` (content unreadable is
    /// reported separately as `RegisteredButUnavailable`).
    RegisteredButUnavailable { report_id: ReportId, reason: String },
    /// B.3 step 4: same `idempotency_key` and same `content_hash` as an
    /// existing Report — the existing `report_id` is returned, no new
    /// Report is created.
    Idempotent { report_id: ReportId },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistrationError {
    #[error("registration request is missing required fields: {0}")]
    IncompleteRequest(String),
    #[error("content_ref could not be constructed: missing content")]
    MissingContentRef,
    #[error("content_hash does not match the submitted content")]
    ContentHashMismatch,
    /// B.3 step 4: same `idempotency_key`, different `content_hash`.
    #[error("idempotency_key '{0}' already used with a different content_hash")]
    IdempotencyKeyConflict(String),
}
