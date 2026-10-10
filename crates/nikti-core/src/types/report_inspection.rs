use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{LogEntryId, ReportId, ReportKind, Timestamp};
use super::report_registration::{ReportState, SourceContext, SourceEventRef};

/// Report Object Model v1.0 (Inspection). Category A evidence (Rule 5 /
/// INV-004): every field is private with a read-only accessor below — no
/// `&mut self` method exists on this type, and none may ever be added.
/// `report_state` and `repository_registered_at` are the only fields that
/// ever differ across the lifetime of one logical report, and they do so
/// only because `ReportRepository::register_inspection_report` constructs
/// a fresh, independent `InspectionReport` value at each lifecycle step
/// and replaces which value its own collection associates with
/// `report_id` — this is a collection-level replacement (the same pattern
/// INV-002 already permits for the Table Repository), never a mutation of
/// a value a caller is already holding. Regeneration always produces a new
/// `report_id`; this type has no method that could update an existing one
/// in place.
///
/// `content_ref`/`content_format`/`content_hash` point at the report's
/// actual content in content-addressed storage — the content itself is
/// deliberately not a field here (see `engines::report::GeneratedInspectionReport`,
/// which pairs one of these with the real `InspectionResults` content on
/// the engine side, above this layer). Keeping content out of this type
/// avoids a types -> engines dependency and matches the frozen "content-
/// addressed Parquet" storage decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionReport {
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

impl InspectionReport {
    /// Intended to be called only from within the Report Engine
    /// (`generate_inspection_report`, INV-015) and the Report Repository's
    /// registration algorithm (B.3) — not a write path against an
    /// existing report; each call produces an independent value.
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
            report_type: ReportKind::Inspection,
            report_title,
            report_state,
            report_schema_version,
            created_at,
            created_by,
            // INV-015: identifies who built the Report object, always the
            // Report Engine — distinct from `source_event_ref.source_engine`,
            // which identifies who produced the evidence being reported.
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

    /// Not a mutation of `self` — produces an independent value carrying a
    /// new `report_state`/`repository_registered_at`, for the repository's
    /// internal collection to associate with `report_id` in place of the
    /// old one. See the type doc comment.
    pub fn with_state(&self, report_state: ReportState, repository_registered_at: Option<Timestamp>) -> Self {
        Self {
            report_state,
            repository_registered_at,
            ..self.clone()
        }
    }
}
