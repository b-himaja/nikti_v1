//! Table Repository Workspace (v2) command-layer support. INV-022:
//! components invoke commands only; INV-023: every UI entry point maps to
//! exactly one canonical command — the 19 commands themselves are
//! declared in `commands::commands`, each a thin (or, for the inherently
//! multi-step `import_table`/`reimport_table_as_new`, documented
//! multi-step) delegation to the REAL logic in this module. This module
//! is where Part H's state-action matrix and the other required
//! behaviors (Parts D, F, G, I) are exercised against actual code, not
//! asserted only in prose — matching the precedent set by
//! `commands::transform_flow_tests`/`commands::import_flow_tests` testing
//! real engine/type/repository logic.

use polars::prelude::{DataFrame, IntoLazy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engines::content::TableContent;
use crate::engines::inspection::{ImportSummary, InspectionEngine, NativeInspectionEngine};
use crate::engines::report::{NativeReportEngine, ReportEngine};
use crate::repositories::table_import::ParquetContentStore;
use crate::repositories::{AnyReport, ReportRepository, RepositoryError, TableRepository};
use crate::types::common::{EngineVersion, ProjectId, ReportKind, TableId, Timestamp};
use crate::types::report_registration::ReportRegistrationRequest;
use crate::types::table::{ColumnDef, TableSchema};
use crate::types::{
    AdminState, CreationSource, Dataset, DescriptiveMetadata, Project, Report, SchemaRef, SourceProvenance, Table,
    TableDataRef, TableEventLogEntry, TableRegistrationError, TableRegistrationOutcome, TableRegistrationRequest,
    TableState, VersionInfo,
};

use super::errors::CommandError;

/// Errors specific to the state-gated Table Workspace actions — distinct
/// from the generic `RepositoryError` so a caller (and the UI) can tell
/// "not found" apart from "found, but not allowed given `table_state`."
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableWorkspaceError {
    #[error("table not found")]
    NotFound,
    #[error("action not allowed for table_state {0:?}")]
    ActionNotAllowed(TableState),
}

impl From<RepositoryError> for TableWorkspaceError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound => TableWorkspaceError::NotFound,
            RepositoryError::Backend(_) => TableWorkspaceError::NotFound,
        }
    }
}

/// Part H's five matrix columns. `RouteInspection`/`RouteValidation`/
/// `RouteAnalysis` share one rule (the matrix has a single "Route
/// Inspection/Validation/Analysis" column, not three) — kept as three
/// variants only so callers can name which route they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableWorkspaceAction {
    OpenReadonly,
    LoadIntoProject,
    RouteInspection,
    RouteValidation,
    RouteAnalysis,
    Archive,
    Restore,
}

/// Part H's full state-action matrix, enforced structurally — every
/// command in Parts F-I that Part H governs must check this before
/// enabling, not just "is a Table selected." `archived_open_policy`
/// answers the matrix's "Only if policy allows" cell for `OpenReadonly`
/// under `Archived`; it is a caller-supplied policy decision, not
/// hardcoded, since the matrix itself defers to policy there.
pub fn table_action_allowed(state: TableState, action: TableWorkspaceAction, archived_open_policy: bool) -> bool {
    use TableState::*;
    use TableWorkspaceAction::*;
    match (state, action) {
        (Registered, _) => false,
        (Available, Restore) => false,
        (Available, _) => true,
        (Archived, OpenReadonly) => archived_open_policy,
        (Archived, Restore) => true,
        (Archived, _) => false,
        (Unavailable, _) => false,
    }
}

/// Part D: a missing `data_dictionary_id` disables only the dictionary
/// action — it is never consulted by `table_action_allowed` and never
/// changes `table_state`.
pub fn dictionary_action_allowed(data_dictionary_id: Option<Uuid>) -> bool {
    data_dictionary_id.is_some()
}

/// A per-action snapshot of `table_action_allowed`'s output for a given
/// Table — exposed to the UI (via `commands::table_allowed_actions`) so
/// enable/disable state can be *reflected* there without a single
/// TypeScript conditional re-deriving the matrix itself (Part C of the
/// Table Repository Workspace UI task). Adds zero new decision logic —
/// every field is the same already-tested `table_action_allowed` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedActions {
    pub open_readonly: bool,
    pub load_into_project: bool,
    pub route_inspection: bool,
    pub route_validation: bool,
    pub route_analysis: bool,
    pub archive: bool,
    pub restore: bool,
}

pub fn allowed_actions(state: TableState, archived_open_policy: bool) -> AllowedActions {
    use TableWorkspaceAction::*;
    AllowedActions {
        open_readonly: table_action_allowed(state, OpenReadonly, archived_open_policy),
        load_into_project: table_action_allowed(state, LoadIntoProject, archived_open_policy),
        route_inspection: table_action_allowed(state, RouteInspection, archived_open_policy),
        route_validation: table_action_allowed(state, RouteValidation, archived_open_policy),
        route_analysis: table_action_allowed(state, RouteAnalysis, archived_open_policy),
        archive: table_action_allowed(state, Archive, archived_open_policy),
        restore: table_action_allowed(state, Restore, archived_open_policy),
    }
}

/// `table.load_into_project` — loads the Table as a Dataset into an
/// EXISTING, already-selected Project. Requires a real `&Project`;
/// creates no Project of its own. Never modifies the Table.
pub fn load_into_project(project: &Project, table: &Table, dataset_id: Uuid, created_at: Timestamp) -> Dataset {
    Dataset::new(
        dataset_id,
        project.id(),
        table.table_name().to_string(),
        Some(table.id()),
        None,
        table.schema().clone(),
        table.row_count(),
        table.table_data_ref().content_ref.clone(),
        table.table_data_ref().data_fingerprint.clone(),
        created_at,
    )
}

/// `table.create_project_from_table` — creates a NEW Project and a
/// Dataset reference to this Table in one action, distinct from
/// `load_into_project` (which requires an existing target Project).
/// Both ultimately construct the Dataset the same way and both reference
/// the Table only by its specific `table_id` (never a version-family
/// "latest") — but the two commands differ in what they require as input
/// and what they return, which is the genuine distinction Part F asks
/// for, not two labels on one call.
pub fn create_project_from_table(new_project_id: ProjectId, project_name: String, dataset_id: Uuid, table: &Table, created_at: Timestamp) -> (Project, Dataset) {
    let project = Project::new(new_project_id, project_name, created_at);
    let dataset = load_into_project(&project, table, dataset_id, created_at);
    (project, dataset)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulesetManagerTarget {
    Validation,
    Analysis,
}

/// `table.route_validation`/`table.route_analysis` (Amendment 13/14) —
/// OPENS the Ruleset Manager or Runtime Manager in Table context; never
/// executes anything itself. Takes no repository handle at all, so
/// writing to the Table Event Log from here is not just avoided by
/// convention — it is structurally unreachable. A `ruleset_version_id`
/// is required before *execution*, not before opening the route — if
/// none is supplied, the navigation says so and the route opens a
/// ruleset-selection step first.
pub fn route_to_ruleset_manager(table_id: TableId, target: RulesetManagerTarget, ruleset_version_id: Option<Uuid>) -> RulesetManagerNavigation {
    RulesetManagerNavigation { table_id, target, ruleset_version_id, requires_ruleset_selection: ruleset_version_id.is_none() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesetManagerNavigation {
    pub table_id: TableId,
    pub target: RulesetManagerTarget,
    pub ruleset_version_id: Option<Uuid>,
    pub requires_ruleset_selection: bool,
}

/// `table.reimport_as_new` — routes to the Import Flow with the existing
/// Table as reference context, but always produces a NEW `table_id`
/// (asserted here, not merely hoped for by the caller) or a new version
/// in the same `version_family_id` — never a mutation of the original.
/// Existing Project references to the original Table are untouched
/// because this never calls any write path against `original` — it only
/// ever registers a brand-new Table.
pub async fn reimport_as_new(
    repo: &impl TableRepository,
    original: &Table,
    request: TableRegistrationRequest,
) -> Result<TableRegistrationOutcome, TableRegistrationError> {
    assert_ne!(request.table_id, original.id(), "reimport_as_new must never reuse the original table_id");
    repo.register_table(request).await
}

/// `table.copy_id`-adjacent rename support (invoked from the Information
/// Inspector, not `table.copy_id` itself). Part A: touches ONLY
/// `descriptive_metadata.table_name` — `table_id`, fingerprints,
/// `content_ref`, and `schema_ref` are structurally untouched because
/// `set_descriptive_metadata` has no way to reach them (see
/// `Table::with_descriptive_metadata`).
pub async fn rename_table(repo: &impl TableRepository, table_id: TableId, new_name: String) -> Result<(), RepositoryError> {
    let table = repo.get(table_id).await?.ok_or(RepositoryError::NotFound)?;
    let updated = DescriptiveMetadata { table_name: new_name, data_dictionary_id: table.descriptive_metadata().data_dictionary_id };
    repo.set_descriptive_metadata(table_id, updated).await
}

/// `table.search` — Table Navigator search over the flattened/indexed
/// fields already derived from the canonical Table object (Part C):
/// `table_name`, `table_id`, `source_label`. Reads only accessors that
/// already exist on `Table` — never a second competing object model.
pub async fn search_tables(repo: &impl TableRepository, query: &str) -> Result<Vec<Table>, RepositoryError> {
    let query = query.to_lowercase();
    let all = repo.list().await?;
    Ok(all
        .into_iter()
        .filter(|t| {
            t.table_name().to_lowercase().contains(&query)
                || t.id().to_string().contains(&query)
                || t.source_provenance().source_label.to_lowercase().contains(&query)
        })
        .collect())
}

/// `table.open_readonly` — Part H: gated by `table_action_allowed`. Never
/// writes to the Table or an existing log entry (INV-018/019) — read-only
/// regardless of outcome.
pub async fn open_table_readonly(repo: &impl TableRepository, table_id: TableId, archived_open_policy: bool) -> Result<Table, TableWorkspaceError> {
    let table = repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?;
    if !table_action_allowed(table.table_state(), TableWorkspaceAction::OpenReadonly, archived_open_policy) {
        return Err(TableWorkspaceError::ActionNotAllowed(table.table_state()));
    }
    Ok(table)
}

/// `table.archive` — Part H: allowed only from `Available`. Non-
/// destructive: replaces only `admin_state`, never Table content.
pub async fn archive_table(repo: &impl TableRepository, table_id: TableId, archived_at: Timestamp) -> Result<Table, TableWorkspaceError> {
    let table = repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?;
    // `archived_open_policy` only governs the OpenReadonly/Archived cell;
    // it is irrelevant to whether Archive itself is allowed, so `false`
    // here does not change the outcome for this action.
    if !table_action_allowed(table.table_state(), TableWorkspaceAction::Archive, false) {
        return Err(TableWorkspaceError::ActionNotAllowed(table.table_state()));
    }
    let mut admin_state = table.admin_state().clone();
    admin_state.table_state = TableState::Archived;
    admin_state.archived_at = Some(archived_at);
    repo.set_admin_state(table_id, admin_state).await?;
    Ok(repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?)
}

/// `table.restore` — Part H: allowed only from `Archived`.
pub async fn restore_table(repo: &impl TableRepository, table_id: TableId) -> Result<Table, TableWorkspaceError> {
    let table = repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?;
    if !table_action_allowed(table.table_state(), TableWorkspaceAction::Restore, false) {
        return Err(TableWorkspaceError::ActionNotAllowed(table.table_state()));
    }
    let mut admin_state = table.admin_state().clone();
    admin_state.table_state = TableState::Available;
    admin_state.archived_at = None;
    repo.set_admin_state(table_id, admin_state).await?;
    Ok(repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?)
}

/// `table.retry_availability_check` — re-attempts whatever made a Table
/// `Unavailable`. The only documented cause of `Unavailable` is Amendment
/// 7's missing `table_event_log_id`; this checks exactly that condition
/// and self-heals to `Available` only if a Table Event Log entry now
/// exists — it never fabricates `Available` on any other basis, and it
/// is a no-op for a Table that isn't `Unavailable`.
pub async fn retry_table_availability_check(repo: &impl TableRepository, table_id: TableId) -> Result<Table, TableWorkspaceError> {
    let table = repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?;
    if table.table_state() != TableState::Unavailable {
        return Ok(table);
    }
    let log = repo.event_log(table_id).await?;
    if let Some(entry) = log.entries().first() {
        let admin_state = AdminState { table_state: TableState::Available, table_event_log_id: Some(entry.id()), archived_at: None };
        repo.set_admin_state(table_id, admin_state).await?;
        return Ok(repo.get(table_id).await?.ok_or(TableWorkspaceError::NotFound)?);
    }
    Ok(table)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictionaryNavigation {
    /// `data_dictionary_id` is `None` — disabled because there is nothing
    /// to open, not because of `table_state`.
    NoDictionaryReferenced,
    /// A `data_dictionary_id` exists, but the Data Dictionary Service
    /// (a separate, future component) doesn't exist yet — Part D:
    /// visibly present, correctly disabled with this state, never
    /// silently missing.
    ServiceNotYetAvailable { data_dictionary_id: Uuid },
}

/// `table.open_dictionary` — Part D: routing only; never edits dictionary
/// content; never affects `table_state` either way.
pub fn open_table_dictionary(table: &Table) -> DictionaryNavigation {
    match table.descriptive_metadata().data_dictionary_id {
        Some(data_dictionary_id) => DictionaryNavigation::ServiceNotYetAvailable { data_dictionary_id },
        None => DictionaryNavigation::NoDictionaryReferenced,
    }
}

fn report_table_id(report: &AnyReport) -> Option<TableId> {
    match report {
        AnyReport::Inspection(r) => r.source_context().table_id,
        AnyReport::Validation(r) => r.source_context().table_id,
        AnyReport::Analysis(r) => r.source_context().table_id,
        // Real as of the Audit Report task — Audit reports are
        // Project/Dataset-sourced (Part D), never Table-sourced, so this
        // is honestly `None` via the same field every other kind uses,
        // not a special case.
        AnyReport::Audit(r) => r.source_context().table_id,
        AnyReport::Lineage(r) => r.source_context().table_id,
        // A Comparison Report's `source_context` is always a neutral
        // placeholder (it's sourced from two other Reports, which
        // `SourceContext` has no variant for) — never a real Table
        // reference, so this is honestly `None`, not a guess.
        AnyReport::Comparison(_) => None,
    }
}

/// `table.open_reports` — lists every Report whose `source_context`/
/// `provenance` references this Table, across all report kinds. Never
/// constructs or edits a Report itself (INV-015) — pure read/filter.
pub async fn open_table_reports(repo: &impl ReportRepository, table_id: TableId) -> Result<Vec<AnyReport>, RepositoryError> {
    let mut result = Vec::new();
    for kind in [ReportKind::Inspection, ReportKind::Validation, ReportKind::Analysis, ReportKind::Audit, ReportKind::Lineage] {
        for report in repo.list_by_kind(kind).await? {
            if report_table_id(&report) == Some(table_id) {
                result.push(report);
            }
        }
    }
    Ok(result)
}

fn schema_to_column_defs(schema: &polars::prelude::Schema) -> Vec<ColumnDef> {
    schema.iter().map(|(name, dtype)| ColumnDef { name: name.to_string(), data_type: format!("{dtype:?}") }).collect()
}

/// The shared core of "take real tabular content and turn it into a
/// registered, evidence-backed Table" — the Amendment 7/10 registration
/// sequence plus the Inspection/Report trigger (Parts B-E of the import-
/// pipeline task). Reused by `commands::import_table` (CSV-sourced) and
/// `commands::project_workspace::export_dataset_as_table` (Dataset-
/// sourced) — there is exactly one implementation of this sequence, not
/// two that could drift apart. Callers differ only in where `frame` came
/// from and what `creation_source`/`source_provenance` describe (per the
/// Table Repository spec's Amendment 4: `import` vs
/// `project_dataset_export`) — never in how registration itself works.
#[allow(clippy::too_many_arguments)]
pub async fn register_content_as_table(
    table_repo: &impl TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    mut frame: DataFrame,
    creation_source: CreationSource,
    source_provenance: SourceProvenance,
    table_name: String,
    idempotency_key: String,
    table_id: TableId,
    version_family_id: Uuid,
    created_at: Timestamp,
    created_by: String,
    malformed_row_count: u64,
) -> Result<Table, CommandError> {
    // Part B: both fingerprints are genuinely content-derived.
    let schema_fingerprint = crate::repositories::table_import::schema_fingerprint(&frame.schema());
    let data_fingerprint =
        crate::repositories::table_import::data_fingerprint(&frame).map_err(|e| CommandError::Repository(RepositoryError::Backend(e.to_string())))?;
    let row_count = frame.height() as u64;
    let columns = schema_to_column_defs(&frame.schema());

    // Part C: content-addressed Parquet — deduplicated by data_fingerprint;
    // never rewritten for content already stored.
    let content_ref = content_store.write_if_absent(&data_fingerprint, &mut frame).map_err(|e| CommandError::Repository(RepositoryError::Backend(e.to_string())))?;

    // Part D: the Amendment 7/10 registration sequence — this function
    // does not re-decide any of its rules, only feeds it real inputs.
    let request = TableRegistrationRequest {
        idempotency_key,
        table_id,
        creation_source,
        source_provenance,
        table_data_ref: TableDataRef { content_ref, data_fingerprint, row_count },
        schema_ref: SchemaRef { schema: TableSchema { columns }, schema_fingerprint },
        version_info: VersionInfo { version_family_id },
        descriptive_metadata: DescriptiveMetadata { table_name, data_dictionary_id: None },
        created_at,
        created_by: created_by.clone(),
    };

    let registration_outcome = table_repo.register_table(request).await.map_err(|e| match e {
        TableRegistrationError::FingerprintPairConflict(key) => CommandError::Repository(RepositoryError::Backend(format!("idempotency_key '{key}' already used with a different fingerprint pair"))),
    })?;

    let registered_table_id = match registration_outcome {
        // Amendment 10: a genuine idempotent hit — nothing new happened,
        // so there is nothing new to log, transition, inspect, or report.
        TableRegistrationOutcome::Idempotent { table_id } => {
            return table_repo.get(table_id).await?.ok_or(CommandError::NotFound);
        }
        TableRegistrationOutcome::Registered { table_id } => table_id,
    };

    // Amendment 7: append the log entry, then transition to Available only
    // if that succeeded — never Available without a table_event_log_id.
    let log_entry_id = Uuid::new_v4();
    let append_result = table_repo.append_event(registered_table_id, TableEventLogEntry::new(log_entry_id, registered_table_id, created_at, "imported".to_string())).await;
    let admin_state = match &append_result {
        Ok(()) => AdminState { table_state: TableState::Available, table_event_log_id: Some(log_entry_id), archived_at: None },
        Err(_) => AdminState { table_state: TableState::Unavailable, table_event_log_id: None, archived_at: None },
    };
    table_repo.set_admin_state(registered_table_id, admin_state).await?;
    let table = table_repo.get(registered_table_id).await?.ok_or(CommandError::NotFound)?;

    // Part E: Inspection + Report still fire unconditionally (INV-011) —
    // their failure never undoes the Table/log above (degraded but
    // recoverable).
    // The exact same fingerprint just registered on `table` — not a
    // separate re-derivation that could theoretically drift from it.
    let content = TableContent { table_id: registered_table_id, content_fingerprint: table.table_data_ref().data_fingerprint.clone(), frame: frame.lazy() };
    let import_summary = ImportSummary { rows_read: row_count, rows_rejected: malformed_row_count, parse_error_count: malformed_row_count, encoding: Some("utf-8".to_string()) };
    let inspection_engine = NativeInspectionEngine;
    if let Ok(results) = inspection_engine.inspect(content, import_summary) {
        let report_engine = NativeReportEngine;
        let generated = report_engine.generate_inspection_report(
            results,
            registered_table_id,
            Uuid::new_v4(),
            EngineVersion { rust_core: "0.1.0".to_string(), python_env_hash: None },
            created_by,
            created_at,
        );
        let content_bytes = serde_json::to_vec(&generated.content).unwrap_or_default();
        let request = ReportRegistrationRequest {
            report_registration_request_id: Uuid::new_v4(),
            idempotency_key: format!("{registered_table_id}:inspection_report:{}", generated.report.content_hash()),
            report_id: generated.report.report_id(),
            report_title: generated.report.report_title().to_string(),
            report_schema_version: generated.report.report_schema_version().to_string(),
            created_at: generated.report.created_at(),
            created_by: generated.report.created_by().to_string(),
            source_context: generated.report.source_context().clone(),
            source_event_ref: generated.report.source_event_ref().clone(),
            content_format: generated.report.content_format().to_string(),
            content_hash: generated.report.content_hash().to_string(),
            content_bytes,
            knowledge_refs: generated.report.knowledge_refs().to_vec(),
            log_refs: vec![log_entry_id],
        };
        let _ = report_repo.register_inspection_report(request).await;
    }

    Ok(table)
}
