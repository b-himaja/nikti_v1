use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type TableId = Uuid;
pub type ReportId = Uuid;
pub type DatasetId = Uuid;
pub type ProjectId = Uuid;
pub type RecipeId = Uuid;
pub type ValidationRulesetId = Uuid;
pub type AnalysisRulesetId = Uuid;
pub type LogEntryId = Uuid;

/// UTC milliseconds since epoch. A plain alias, not a wall-clock guarantee —
/// evidence timestamps are recorded by the engine/command layer, never by the UI.
pub type Timestamp = i64;

/// INV-016: reports may only be compared same-type-to-same-type.
///
/// `EnumIter` (status audit 3, Part B): `ReportKind::iter()` gives every
/// test/scan site a way to enumerate "all report kinds" that is derived
/// from this enum's real definition rather than a second hand-typed
/// list — the exhaustiveness test in `report_audit_tests.rs` uses this
/// instead of a literal array so a 7th variant is automatically included
/// with no separate list to remember to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumIter)]
pub enum ReportKind {
    Inspection,
    Validation,
    Analysis,
    Audit,
    Lineage,
    Comparison,
}

/// INV-001: every result traces to source data, operations, rulesets, and the
/// engine (+ compute backend) that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRef {
    pub source_table_id: Option<TableId>,
    pub source_dataset_id: Option<DatasetId>,
    pub ruleset_id: Option<Uuid>,
    pub recipe_id: Option<RecipeId>,
    pub operation_log_entry_id: Option<LogEntryId>,
}

/// INV-030: a report produced via the Python sidecar records a composite
/// engine_version, not just the Rust core version, so a dependency bump is
/// visible in downstream provenance rather than silently absorbed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineVersion {
    pub rust_core: String,
    pub python_env_hash: Option<String>,
}
