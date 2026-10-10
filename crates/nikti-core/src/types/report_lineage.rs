use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{DatasetId, LogEntryId, RecipeId, ReportId, ReportKind, TableId, Timestamp};
use super::report_registration::{ReportState, SourceContext, SourceEventRef};

/// A single addressable object a lineage graph can reference. Kept to
/// exactly the three kinds this data model actually has independently
/// addressable identity for — there is no fourth "Project" node type
/// because a Project is never itself a lineage endpoint anywhere else in
/// this app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LineageNode {
    ImportedTable(TableId),
    Dataset(DatasetId),
    Report(ReportId),
}

/// Every edge here corresponds to exactly one real, checkable fact
/// already sitting in this project's data model — see the Lineage
/// Report task's own Output for the field each variant is built from.
/// Two notes on why this isn't literally the 5-type list a prior
/// product-spec description named:
///
/// - `DatasetDuplicated` is an addition beyond that list.
///   `Dataset.source_dataset_id` is real and traceable, but its ONLY
///   real producer anywhere in the command layer is `duplicate_dataset`
///   — `run_transformation`/`apply_recipe_to_dataset` mutate a Dataset
///   in place (`Dataset::apply_change`) and never create a new
///   `DatasetId`. Labeling a duplicate-chain link as `TransformationRun`
///   or `RecipeApplied` would misrepresent what actually happened, which
///   this task's own central rule forbids more than omitting a type
///   name does.
/// - `RecipeApplied` is kept (per the requested vocabulary) but is
///   still structurally unreachable from the real traversal in this
///   codebase — for a different reason than it originally was. As of
///   the Audit Report task's Part A, `apply_recipe_to_dataset` *does*
///   record a real `recipe_id`/`recipe_version` on the resulting
///   `ProjectOperationLogEntry` (`new_recipe_transformation`) — that
///   data now genuinely exists, and the earlier claim that it didn't is
///   stale. The actual remaining obstacle is `RecipeApplied`'s own
///   shape: it requires two *distinct* `DatasetId`s
///   (`from_dataset_id`/`to_dataset_id`), and neither
///   `run_transformation` nor `apply_recipe_to_dataset` has ever created
///   a second `DatasetId` — both mutate the target Dataset in place
///   (`Dataset::apply_change`), the same reason `TransformationRun` is a
///   self-referencing edge below. A recipe-driven run has a real
///   `recipe_id` to attach to *something*, but nothing in this data
///   model to attach a two-node edge to; that's a smaller, different gap
///   than "no data exists," and status audit 3 flagged it explicitly as
///   worth re-checking before this comment is touched again. See
///   `no_transformation_edge_is_ever_labeled_recipe_applied` in the
///   test suite (still accurate — the edge is still never produced —
///   only the reasoning above has changed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineageEdge {
    /// `Dataset.source_table_id` — real.
    LoadIntoProject { table_id: TableId, dataset_id: DatasetId },
    /// One real `ProjectOperationLogEntry` with
    /// `transformation_operation.is_some()`, on the same Dataset
    /// (`run_transformation` mutates in place — there is no second
    /// Dataset node to point at, so this is a genuine self-referencing
    /// edge, not a fabricated second node).
    TransformationRun { dataset_id: DatasetId, operation_log_entry_id: LogEntryId, operation: String },
    /// Structurally unreachable today — see this enum's own doc comment.
    RecipeApplied { from_dataset_id: DatasetId, to_dataset_id: DatasetId, recipe_id: RecipeId },
    /// `Dataset.source_dataset_id` — real; see this enum's own doc
    /// comment for why this isn't folded into `TransformationRun`.
    DatasetDuplicated { from_dataset_id: DatasetId, to_dataset_id: DatasetId },
    /// A real `AnyReport` whose `source_context` resolves to `source`.
    GeneratedReport { source: LineageNode, report_id: ReportId },
    /// `Table.source_provenance.source_dataset_id`, populated for real by
    /// `export_dataset_as_table` — see this task's Output for the
    /// confirmation this field is genuinely set, not just described as
    /// such.
    ExportAsTable { dataset_id: DatasetId, table_id: TableId },
}

/// INV-001: the full provenance graph for a Table or Dataset — every
/// node and edge here is resolved from an already-real, already-tracked
/// reference; nothing is computed or inferred. See
/// `commands::generate_lineage_report` for the traversal that builds
/// this — this type only describes the resulting shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageContent {
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
    /// A real count of edges traversed backward (toward origins) from
    /// the query object — never an estimate.
    pub upstream_count: u64,
    /// `None`, not `0`: forward (downstream) traversal is explicitly
    /// out of scope for this task (see its Output) — no efficient
    /// reverse index exists for "what was produced from this node," and
    /// reporting `0` would falsely claim a confirmed absence of
    /// downstream consumers rather than "not checked."
    pub downstream_count: Option<u64>,
    /// Any node the backward traversal reached that turned out to be
    /// archived or purged — traversal stops at that node rather than
    /// following any reference past it (Part C). Only `ImportedTable`
    /// nodes can appear here: neither `Dataset` nor `Report` has an
    /// archived/purged concept anywhere in this data model.
    pub archived_or_purged: Vec<LineageNode>,
    /// `true` only when the entire traversed graph is intact and
    /// loadable: no archived/purged node encountered, and no edge left
    /// unresolved. `false` otherwise — never asserted `true` by default.
    pub reproducible: bool,
}

/// Category A evidence (Rule 5 / INV-004): no write method, ever. Same
/// baseline shape as `InspectionReport`/`AnalysisReport` (Report Object
/// Model v1.0) — not the older bare `ReportMeta`+content shape this type
/// used before this task, which had no `report_state`/`source_context`/
/// `source_event_ref` and so could never go through the same idempotent,
/// state-machine-gated registration flow every other report kind uses.
/// Safe to change field-for-field here: `insert_lineage` was `todo!()`
/// and no `LineageReport` value has ever actually been constructed or
/// persisted anywhere in this codebase, so there is no existing evidence
/// instance this change could be mutating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageReport {
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

impl LineageReport {
    /// Intended to be called only from within the Report Engine
    /// (`generate_lineage_report`, INV-015) and the Report Repository's
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
            report_type: ReportKind::Lineage,
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
