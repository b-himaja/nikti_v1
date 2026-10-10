use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{LogEntryId, ReportId, ReportKind, Timestamp};
use super::report_registration::{ReportState, SourceContext, SourceEventRef};

/// One real, structural difference between two content trees, found by
/// walking both `serde_json::Value` bodies together — never a
/// synthesized "looks different" heuristic. `path` is a JSON-Pointer-
/// style dotted path (`"results.rule_results.0.failed_count"`) so a
/// reader can locate exactly where in each report's content the
/// difference sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiffEntry {
    Added { path: String, value: serde_json::Value },
    Removed { path: String, value: serde_json::Value },
    Changed { path: String, before: serde_json::Value, after: serde_json::Value },
}

/// Part C: genuinely incomparable content is a distinct, honest outcome
/// — never silently diffed as if it were compatible. `Diffed(vec![])`
/// (not a separate variant) is what "no differences" looks like — an
/// empty diff is still a completed, meaningful comparison, unlike a
/// schema mismatch, which never got that far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComparisonOutcome {
    Diffed(Vec<DiffEntry>),
    SchemaVersionMismatch { report_a_schema_version: String, report_b_schema_version: String },
}

/// INV-001: every value here traces to a real structural comparison of
/// the two real reports' real content — see
/// `commands::generate_comparison_report`'s own doc comment for exactly
/// where that content comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonContent {
    pub report_a_id: ReportId,
    pub report_b_id: ReportId,
    pub report_kind: ReportKind,
    pub outcome: ComparisonOutcome,
}

/// Category A evidence (Rule 5 / INV-004): no write method, ever. Same
/// baseline shape as every other report kind, plus two extra fields —
/// same precedent `ValidationReport` already establishes for a report
/// kind that genuinely needs something beyond the shared baseline
/// (`report_template_version`/`source_validation_result_id`/
/// `execution_id`). Here: `SourceContext` has no variant for "this
/// report's source is two other Reports" (only Table/ProjectDataset/
/// Project), so the two compared report ids are carried explicitly
/// rather than forced into a field that doesn't really mean anything
/// for this kind — `source_context` on a `ComparisonReport` is always a
/// neutral placeholder (see `generate_comparison_report`'s own doc
/// comment), never treated as if it identified something real.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
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
    compared_report_a_id: ReportId,
    compared_report_b_id: ReportId,
}

impl ComparisonReport {
    /// Intended to be called only from within the Report Engine
    /// (`generate_comparison_report`, INV-015) and the Report
    /// Repository's registration algorithm (B.3).
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
        compared_report_a_id: ReportId,
        compared_report_b_id: ReportId,
    ) -> Self {
        Self {
            report_id,
            report_type: ReportKind::Comparison,
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
            compared_report_a_id,
            compared_report_b_id,
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

    pub fn compared_report_a_id(&self) -> ReportId {
        self.compared_report_a_id
    }

    pub fn compared_report_b_id(&self) -> ReportId {
        self.compared_report_b_id
    }

    /// Not a mutation of `self` — see `InspectionReport::with_state`.
    pub fn with_state(&self, report_state: ReportState, repository_registered_at: Option<Timestamp>) -> Self {
        Self { report_state, repository_registered_at, ..self.clone() }
    }
}

/// A real, recursive structural diff between two `serde_json::Value`
/// trees — the same generic, by-key approach the Report Viewer already
/// established as honest given report content isn't typed per kind
/// anywhere in this codebase. Walks objects key-by-key and arrays
/// index-by-index; any other type change (or a scalar value change) is
/// `Changed`. Deterministic key ordering (`BTreeMap`-free, but
/// `serde_json::Map` iterates in insertion order and both inputs came
/// from the same serializer, so this is stable across repeated calls
/// with unchanged inputs — required for Part D's idempotency).
pub fn structural_diff(path: &str, a: &serde_json::Value, b: &serde_json::Value) -> Vec<DiffEntry> {
    use serde_json::Value;

    match (a, b) {
        (Value::Object(map_a), Value::Object(map_b)) => {
            let mut entries = Vec::new();
            for (key, value_a) in map_a {
                let child_path = format!("{path}.{key}");
                match map_b.get(key) {
                    Some(value_b) => entries.extend(structural_diff(&child_path, value_a, value_b)),
                    None => entries.push(DiffEntry::Removed { path: child_path, value: value_a.clone() }),
                }
            }
            for (key, value_b) in map_b {
                if !map_a.contains_key(key) {
                    entries.push(DiffEntry::Added { path: format!("{path}.{key}"), value: value_b.clone() });
                }
            }
            entries
        }
        (Value::Array(arr_a), Value::Array(arr_b)) => {
            let mut entries = Vec::new();
            let max_len = arr_a.len().max(arr_b.len());
            for i in 0..max_len {
                let child_path = format!("{path}[{i}]");
                match (arr_a.get(i), arr_b.get(i)) {
                    (Some(va), Some(vb)) => entries.extend(structural_diff(&child_path, va, vb)),
                    (Some(va), None) => entries.push(DiffEntry::Removed { path: child_path, value: va.clone() }),
                    (None, Some(vb)) => entries.push(DiffEntry::Added { path: child_path, value: vb.clone() }),
                    (None, None) => unreachable!(),
                }
            }
            entries
        }
        (a, b) if a == b => Vec::new(),
        (a, b) => vec![DiffEntry::Changed { path: path.to_string(), before: a.clone(), after: b.clone() }],
    }
}
