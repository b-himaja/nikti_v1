use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{LogEntryId, ReportId, ReportKind, Timestamp};
use super::report_registration::{ReportState, SourceContext, SourceEventRef};

/// Report Object Model v1.0 (Analysis), Part F. Same immutability
/// contract, same nested `source_context`/`source_event_ref`, same
/// content-not-embedded shape as `InspectionReport` — see that type's
/// doc comment for the full reasoning, not repeated here. Part F names
/// no Analysis-specific top-level fields beyond the shared baseline
/// (unlike Validation's `report_template_version`/
/// `source_validation_result_id`/`execution_id` — those were an
/// explicit Part D addition for that report kind specifically; nothing
/// analogous was requested here, so this type is not over-built with
/// invented fields). `knowledge_refs` carries exactly one entry: the
/// specific `AnalysisRulesetVersion` used — never a bare ruleset id
/// without its version, matching the pattern already established for
/// Validation Reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
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

impl AnalysisReport {
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
            report_type: ReportKind::Analysis,
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
