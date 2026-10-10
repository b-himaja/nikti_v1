use std::path::Path;

use uuid::Uuid;

use crate::commands::{project_workspace, table_workspace};
use crate::engines::{NativeReportEngine, ReportEngine};
use crate::repositories::table_import::read_csv;
use crate::repositories::{DatasetRepository, DetailTableStore, KnowledgeRepository, ParquetContentStore, PendingPlanRepository, ProjectRepository, ReportRepository, TableRepository};
use crate::sidecar::ComputeBackend;
use crate::types::common::{
    AnalysisRulesetId, DatasetId, LogEntryId, ProjectId, RecipeId, ReportId, TableId, Timestamp,
    ValidationRulesetId,
};
use crate::types::{
    AnalysisReport, CreationSource, CreationSourceType, Dataset, InspectionReport, LineageContent, LineageEdge, LineageNode,
    PendingTransformationPlan, Project, ProjectOperationLogEntry, Recipe, RegistrationOutcome, ReportKind, ReportRegistrationRequest, SourceContext, SourceContextKind, SourceProvenance, SourceType, Table,
    TableEventLog, TableState, TransformationOperation, ValidationReport,
};

use super::errors::CommandError;
use crate::repositories::AnyReport;

/// Where `import_table` reads raw content from — CSV first (Part A of the
/// import-pipeline task); other formats are a later task's variants.
pub struct ImportSource {
    pub uri: String,
}

fn table_name_from_uri(uri: &str) -> String {
    Path::new(uri).file_stem().and_then(|s| s.to_str()).unwrap_or("table").to_string()
}

/// Steps (1)-(5) of `import_table`'s documented sequence, shared by
/// `import_table` and `reimport_table_as_new` (Part F: the same pipeline,
/// just with a caller-supplied `idempotency_key`/`table_id` and,
/// optionally, a `version_family_id` inherited from an original Table —
/// never a second, parallel implementation of this sequence).
#[allow(clippy::too_many_arguments)]
async fn run_import_pipeline(
    table_repo: &impl TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    source: &ImportSource,
    idempotency_key: String,
    table_id: TableId,
    version_family_id: Uuid,
    created_at: Timestamp,
    created_by: String,
) -> Result<Table, CommandError> {
    // Part A: real lazy CSV scan + collect (streaming above the configured
    // threshold) — never silently drops/coerces malformed rows; their
    // count is recorded, not discarded.
    let path = Path::new(&source.uri);
    let outcome = read_csv(path).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;

    // Parts B-E (fingerprinting, content-addressed storage, the
    // Amendment 7/10 registration sequence, and the Inspection/Report
    // trigger) are shared with `project_workspace::export_dataset_as_table`
    // — see `table_workspace::register_content_as_table`'s doc comment for
    // why this is one implementation, not two.
    table_workspace::register_content_as_table(
        table_repo,
        report_repo,
        content_store,
        outcome.frame,
        CreationSource { creation_source_type: CreationSourceType::Import, source_type: SourceType::Csv },
        SourceProvenance { source_label: source.uri.clone(), source_project_id: None, source_dataset_id: None },
        table_name_from_uri(&source.uri),
        idempotency_key,
        table_id,
        version_family_id,
        created_at,
        created_by,
        outcome.malformed_row_count,
    )
    .await
}

/// Rule 9 / frozen sidecar contract: `analyse_python` cannot always produce
/// a report — a down/timed-out/crashed/unimplemented sidecar resolves to
/// `NotExecuted` with a reason, never a partial `AnalysisReport`.
pub enum AnalysisCommandOutcome {
    Completed(AnalysisReport),
    NotExecuted { reason: String },
}

/// INV-023: every user-accessible action maps to exactly one canonical
/// command below; menus, toolbars, shortcuts, and buttons all invoke the
/// same function, never a separate implementation per UI surface.
/// INV-022: components call these, never engines or repositories directly.
/// INV-010: cross-engine work is routed through this layer.

/// INV-011/INV-023: on successful import, this command must run, as one
/// canonical sequence (never three separately user-triggered actions):
/// (1) register the Table via `TableRepository::register_table`
///     (Amendment 10: idempotency is checked on the
///     `data_fingerprint`+`schema_fingerprint` *pair*, never
///     `data_fingerprint` alone — a same-key/different-schema retry is
///     rejected, not silently deduplicated), inserted at
///     `TableState::Registered`,
/// (2) append a `TableEventLogEntry` recording the import,
/// (3) transition the Table's `admin_state` via
///     `TableRepository::set_admin_state` — Amendment 7: `Available` only
///     if (2) succeeded, with `table_event_log_id` set to that entry's
///     id; `Unavailable` (no log id) if (2) failed — a Table must never
///     reach `Available` without a `table_event_log_id`,
/// (4) invoke `NativeInspectionEngine::inspect` unconditionally,
/// (5) build the report via `NativeReportEngine::generate_inspection_report`
///     and register it via `ReportRepository::register_inspection_report`.
///
/// Steps (1)-(3) succeeding is what makes the import itself successful —
/// this function's `Result` reflects only that. Steps (4)+(5) failing is
/// a degraded-but-recoverable state, never a reason to fail the whole
/// import or roll back (1)-(3): the Table exists, is valid, and is
/// logged either way. See `commands::import_flow_tests` for this exact
/// sequence, including both failure modes and the Part 0 consistency
/// checks (Amendment 7/10), exercised against real (non-`todo!()`)
/// engine/type/repository-test-double code.
///
/// Real, end to end: reads `source.uri` as CSV (Part A), computes both
/// fingerprints from actual content (Part B), writes/dedups content-
/// addressed Parquet (Part C), and hands off to the already-corrected
/// registration sequence (Part D) — same `idempotency_key` (here, the
/// source URI itself: re-importing the same file twice must hit the
/// idempotency path, per Amendment 10) across repeated calls is what
/// makes retried imports of the same file idempotent rather than
/// duplicative.
pub async fn import_table(
    table_repo: &impl TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    source: ImportSource,
    created_at: Timestamp,
    created_by: String,
) -> Result<Table, CommandError> {
    let idempotency_key = source.uri.clone();
    run_import_pipeline(table_repo, report_repo, content_store, &source, idempotency_key, Uuid::new_v4(), Uuid::new_v4(), created_at, created_by).await
}

/// `table.reimport_as_new` (Part I/Part F) — routes to the Import Flow
/// with `original_table_id` as reference context. Reads `original_table_id`
/// ONCE, read-only, only to inherit its `version_family_id`; never writes
/// to it, never re-checks its fingerprints, never touches its Project
/// references. Runs the identical Parts A-D pipeline against `source`
/// with a freshly generated `table_id` and a freshly generated
/// `idempotency_key` (never the original's), so this always produces a
/// new Table registration, even if `source`'s content is byte-identical
/// to the original — the content store still dedups the Parquet write
/// (Part C), but the Table record is always new (Part F).
pub async fn reimport_table_as_new(
    table_repo: &impl TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    original_table_id: TableId,
    source: ImportSource,
    created_at: Timestamp,
    created_by: String,
) -> Result<Table, CommandError> {
    let original = table_repo.get(original_table_id).await?.ok_or(CommandError::NotFound)?;
    let version_family_id = original.version_info().version_family_id;
    let idempotency_key = format!("reimport:{original_table_id}:{}", Uuid::new_v4());
    run_import_pipeline(table_repo, report_repo, content_store, &source, idempotency_key, Uuid::new_v4(), version_family_id, created_at, created_by).await
}

/// `table.search` — Table Navigator search, over the flattened/indexed
/// fields derived from the canonical Table object (Part C) only, never a
/// second competing object model.
pub async fn search_tables(table_repo: &impl TableRepository, query: String) -> Result<Vec<Table>, CommandError> {
    Ok(table_workspace::search_tables(table_repo, &query).await?)
}

/// `table.clear_search` — returns the Navigator to its unfiltered view.
pub async fn clear_table_search(table_repo: &impl TableRepository) -> Result<Vec<Table>, CommandError> {
    Ok(table_repo.list().await?)
}

/// `table.refresh_repository` — re-reads the Table Navigator's list/index
/// views (`All Available Tables`, `Recent Tables`, `Archived Tables`,
/// `Registered/Unavailable Tables`). Never writes anything (INV-018).
pub async fn refresh_table_repository(table_repo: &impl TableRepository) -> Result<Vec<Table>, CommandError> {
    Ok(table_repo.list().await?)
}

/// `table.open_readonly` — displays Table data read-only. Part H: gated
/// by `table_workspace::table_action_allowed(state, TableWorkspaceAction::OpenReadonly, ..)`;
/// never writes to the Table or an existing log entry (INV-018/019).
pub async fn open_table_readonly(table_repo: &impl TableRepository, table_id: TableId, archived_open_policy: bool) -> Result<Table, CommandError> {
    Ok(table_workspace::open_table_readonly(table_repo, table_id, archived_open_policy).await?)
}

/// `table.open_information` — "Open Table Information" (Part A: not
/// "Properties"/"Open Table Metadata"). Unconditionally available given a
/// selected Table, regardless of `table_state` — the Information
/// Inspector is metadata display, not the Part H matrix's "Open
/// read-only" data view.
pub async fn open_table_information(table_repo: &impl TableRepository, table_id: TableId) -> Result<Table, CommandError> {
    table_repo.get(table_id).await?.ok_or(CommandError::NotFound)
}

/// `table.open_dictionary` — routes to the (separate, future) Data
/// Dictionary Service. Part D: disabled with a "not yet available" state
/// when `data_dictionary_id` is `None` or the service doesn't exist yet —
/// never silently missing, and never affects `table_state`.
pub async fn open_table_dictionary(table_repo: &impl TableRepository, table_id: TableId) -> Result<table_workspace::DictionaryNavigation, CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    Ok(table_workspace::open_table_dictionary(&table))
}

pub async fn open_table_event_log(table_repo: &impl TableRepository, table_id: TableId) -> Result<TableEventLog, CommandError> {
    Ok(table_repo.event_log(table_id).await?)
}

pub async fn open_table_reports(report_repo: &impl ReportRepository, table_id: TableId) -> Result<Vec<AnyReport>, CommandError> {
    Ok(table_workspace::open_table_reports(report_repo, table_id).await?)
}

/// The Report Viewer task's own command-layer piece: `ReportRepository::
/// get(id)` (pre-existing) resolves only a Report's metadata baseline —
/// nothing anywhere resolved the actual content a `content_ref` points
/// at. This is the one canonical command every Report Viewer instance
/// calls, regardless of which workspace opened it (Table Repository's
/// Reports tab, Project Workspace's Dataset Reports tab, and eventually
/// the Report Repository workspace) — INV-022/023: one command, reused,
/// never a per-workspace variant.
///
/// Content is returned as parsed JSON rather than a typed struct per
/// report kind: each report kind's body has its own distinct shape
/// (`rule_results` for Validation, `procedure_results` for Analysis,
/// etc. — see `report_validation.rs`/`report_analysis.rs`), and building
/// three separate typed content models is a larger, separate task than
/// "make report content readable at all." `serde_json::Value` here is
/// the same honest, generic representation `detail_storage_tests.rs`
/// already uses to assert on real report content.
pub async fn open_report(report_repo: &impl ReportRepository, report_id: ReportId) -> Result<ReportWithContent, CommandError> {
    let report = report_repo.get(report_id).await?.ok_or(CommandError::NotFound)?;
    let content = fetch_report_content(report_repo, &report).await?;
    Ok(ReportWithContent { report, content })
}

/// Shared by `open_report` (Report Viewer) and `generate_comparison_report`
/// below — the one place this codebase resolves a Report's real content
/// bytes into parsed JSON, reused rather than duplicated.
fn content_ref_of(report: &AnyReport) -> Result<&str, CommandError> {
    match report {
        AnyReport::Inspection(r) => Ok(r.content_ref()),
        AnyReport::Validation(r) => Ok(r.content_ref()),
        AnyReport::Analysis(r) => Ok(r.content_ref()),
        AnyReport::Lineage(r) => Ok(r.content_ref()),
        AnyReport::Comparison(r) => Ok(r.content_ref()),
        // Real as of the Audit Report task — this match was never
        // updated when Lineage/Comparison became real, the exact class
        // of bug Part E of this task added a dedicated test for
        // (`every_reportkind_variant_is_openable_via_open_report`).
        AnyReport::Audit(r) => Ok(r.content_ref()),
    }
}

async fn fetch_report_content(report_repo: &impl ReportRepository, report: &AnyReport) -> Result<serde_json::Value, CommandError> {
    let content_ref = content_ref_of(report)?;
    let bytes = report_repo.get_content(content_ref).await?;
    serde_json::from_slice(&bytes).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))
}

/// The Report Viewer's response shape — real metadata (`AnyReport`,
/// exactly what `open_table_reports` already returns per entry) plus the
/// real parsed content body. Never constructed anywhere except by
/// `open_report` above — no workspace/component builds one itself.
#[derive(Debug, serde::Serialize)]
pub struct ReportWithContent {
    pub report: AnyReport,
    pub content: serde_json::Value,
}

/// `table.load_into_project` — loads the Table as a Dataset into an
/// EXISTING, already-selected Project. Distinct from
/// `create_project_from_table_command` (Part F correction: two commands,
/// not one) — this one requires the caller to supply a `project_id` that
/// already resolves via `ProjectRepository::get`, and never constructs a
/// Project. See `table_workspace::load_into_project` for the real Dataset-
/// construction logic; gated by Part H's matrix.
///
/// BUG FIX (Project Workspace UI wiring task, Part B): this function
/// constructed a `Dataset` and recorded its id on the Project, but never
/// took a `dataset_repo` parameter at all, so the Dataset itself was
/// never persisted to `DatasetRepository` — `project.dataset_ids()` would
/// contain an id `DatasetRepository::get` could never resolve, silently
/// breaking every subsequent Dataset-targeted command
/// (transform/duplicate/export/validate/analyse) on it. Found while
/// verifying the Table Repository -> Project Workspace hand-off actually
/// works end to end, not assumed from the two sides' separately-passing
/// tests. Fixed here rather than left for a future task: the intended
/// behavior was never ambiguous (every other Dataset-creating path in
/// this same file inserts into `dataset_repo`; this was the one
/// outlier), so this is a root-cause correction of a found defect, not
/// new business logic.
pub async fn load_table_into_project(
    table_repo: &impl TableRepository,
    project_repo: &impl ProjectRepository,
    dataset_repo: &impl DatasetRepository,
    table_id: TableId,
    project_id: ProjectId,
    created_at: Timestamp,
) -> Result<Dataset, CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    if !table_workspace::table_action_allowed(table.table_state(), table_workspace::TableWorkspaceAction::LoadIntoProject, false) {
        return Err(table_workspace::TableWorkspaceError::ActionNotAllowed(table.table_state()).into());
    }
    let mut project = project_repo.get(project_id).await?.ok_or(CommandError::NotFound)?;
    let dataset = table_workspace::load_into_project(&project, &table, Uuid::new_v4(), created_at);
    dataset_repo.insert(dataset.clone()).await?;
    project.add_dataset(dataset.id());
    // Stale Comment Fix + Load-Path Gating Gap task: this call previously
    // never logged the load event at all, unlike the equivalent
    // `project_workspace::load_table_as_dataset` (`operation: "load"`) —
    // a genuine evidence-completeness gap (INV-001/INV-013's spirit:
    // materializing a Dataset from a Table is exactly the kind of
    // command-layer event the Project Operation Log exists to record),
    // not a cosmetic one. Same `"load"` operation string as that
    // function, so both real "load a table into a project" paths now
    // produce the same kind of log entry.
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), project_id, dataset.id(), created_at, "load".to_string()));
    project_repo.save(project).await?;
    Ok(dataset)
}

/// `table.create_project_from_table` — creates a NEW Project and a
/// Dataset reference to this Table in one action. Distinct from
/// `load_table_into_project` — this one takes only a name, never an
/// existing `project_id`, and always inserts a brand-new Project. See
/// `table_workspace::create_project_from_table`.
///
/// Same bug fix as `load_table_into_project` above, same reason: the
/// Dataset it constructs was never persisted to `DatasetRepository`.
pub async fn create_project_from_table_command(
    table_repo: &impl TableRepository,
    project_repo: &impl ProjectRepository,
    dataset_repo: &impl DatasetRepository,
    table_id: TableId,
    new_project_name: String,
    created_at: Timestamp,
) -> Result<(Project, Dataset), CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    if !table_workspace::table_action_allowed(table.table_state(), table_workspace::TableWorkspaceAction::LoadIntoProject, false) {
        return Err(table_workspace::TableWorkspaceError::ActionNotAllowed(table.table_state()).into());
    }
    let (mut project, dataset) = table_workspace::create_project_from_table(Uuid::new_v4(), new_project_name, Uuid::new_v4(), &table, created_at);
    dataset_repo.insert(dataset.clone()).await?;
    project.add_dataset(dataset.id());
    // Same gap `load_table_into_project` had (Stale Comment Fix + Load-
    // Path Gating Gap task) and the same fix — but NOT the same
    // operation label. The Dataset-level fact ("a Table was loaded to
    // produce this Dataset") is identical between the two commands, but
    // this command's own doc comment above already treats it as
    // "distinct from `load_table_into_project`" for a real reason: this
    // is the founding event of a brand-new Project (no prior history to
    // load into), not one load among an existing Project's history — a
    // reader of this Project's very first log entry benefits from being
    // able to tell "this project was created from a table" apart from
    // "a table was loaded into an established project" without having
    // to separately check whether it's entry #1. No existing code reads
    // `operation()`'s string value to branch behavior (checked directly
    // — Lineage's `LoadIntoProject` edge derives from
    // `Dataset.source_table_id()`, never from this string), so choosing
    // a distinct label here doesn't risk breaking anything that already
    // depends on "load".
    let new_project_id = project.id();
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), new_project_id, dataset.id(), created_at, "create_project_from_table".to_string()));
    project_repo.insert(project.clone()).await?;
    Ok((project, dataset))
}

/// `table.route_inspection` — a convenience re-run wrapper around
/// `inspect_table`; same command, not a second code path (unchanged from
/// the earlier version of this prompt). `inspect_table` itself remains
/// `todo!()` — implementing it for real is that command's own task, not
/// this one's; this function correctly inherits that status rather than
/// working around it with a second code path.
pub async fn route_table_inspection(table_id: TableId) -> Result<InspectionReport, CommandError> {
    inspect_table(table_id).await
}

/// `table.route_validation`/`table.route_analysis` (Amendment 13/14) —
/// OPENS the Ruleset/Runtime Manager in Table context; never executes
/// anything and never writes a Table Event Log entry by itself — only
/// actual execution (`validate_table`/`analyse_native`/`analyse_python`)
/// does. Takes no repository handle at all (see
/// `table_workspace::route_to_ruleset_manager`), so it cannot write
/// evidence even by accident.
pub async fn route_table_validation(table_id: TableId, ruleset_version_id: Option<ValidationRulesetId>) -> Result<table_workspace::RulesetManagerNavigation, CommandError> {
    Ok(table_workspace::route_to_ruleset_manager(table_id, table_workspace::RulesetManagerTarget::Validation, ruleset_version_id))
}

pub async fn route_table_analysis(table_id: TableId, ruleset_version_id: Option<AnalysisRulesetId>) -> Result<table_workspace::RulesetManagerNavigation, CommandError> {
    Ok(table_workspace::route_to_ruleset_manager(table_id, table_workspace::RulesetManagerTarget::Analysis, ruleset_version_id))
}

/// `table.archive` — Part H: allowed only from `Available`. Non-
/// destructive: never touches Table content, only `admin_state` (see
/// `TableRepository::set_admin_state`).
pub async fn archive_table(table_repo: &impl TableRepository, table_id: TableId, archived_at: Timestamp) -> Result<Table, CommandError> {
    Ok(table_workspace::archive_table(table_repo, table_id, archived_at).await?)
}

/// `table.restore` — Part H: allowed only from `Archived`.
pub async fn restore_table(table_repo: &impl TableRepository, table_id: TableId) -> Result<Table, CommandError> {
    Ok(table_workspace::restore_table(table_repo, table_id).await?)
}

/// `table.retry_availability_check` — re-attempts whatever made a Table
/// `Unavailable` (e.g. a missing `table_event_log_id`, Amendment 7);
/// never fabricates `Available` without satisfying the same ordering
/// `import_table` enforces.
pub async fn retry_table_availability_check(table_repo: &impl TableRepository, table_id: TableId) -> Result<Table, CommandError> {
    Ok(table_workspace::retry_table_availability_check(table_repo, table_id).await?)
}

pub async fn copy_table_id(table_id: TableId) -> Result<String, CommandError> {
    Ok(table_id.to_string())
}

/// Not one of the 19 canonical `table.*` action commands — a read-only
/// query the UI uses to *reflect* Part H's state-action matrix (disabled
/// buttons, tooltips) without any TypeScript conditional re-deriving the
/// rule. Adds no new decision logic: it fetches the Table and returns
/// `table_workspace::allowed_actions`, the same already-tested function
/// every gated command above calls internally.
pub async fn table_allowed_actions(table_repo: &impl TableRepository, table_id: TableId, archived_open_policy: bool) -> Result<table_workspace::AllowedActions, CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    Ok(table_workspace::allowed_actions(table.table_state(), archived_open_policy))
}

/// AMBIGUOUS — neither wired nor a straightforward genuine gap; see this
/// task's Output. INV-018/019: Open = display; never writes to a Table
/// or log entry. This command predates the Table Repository Workspace
/// task's Part A naming correction, which replaced this single generic
/// concept with two distinct, more precise commands:
/// `open_table_readonly` (data view, state-gated) and
/// `open_table_information` (metadata view, ungated) — both real and
/// wired. There is no way to tell which one `open_table`'s callers meant
/// without guessing at product intent, and this command has zero call
/// sites in the current UI (confirmed: `src/utils/ipc.ts`/`src/workspaces`
/// only ever call the two specific successors). Left as `todo!()` rather
/// than arbitrarily picking one.
pub async fn open_table(_table_id: TableId) -> Result<Table, CommandError> {
    todo!()
}

/// GENUINE GAP — see this task's Output. The Inspection Engine and
/// automatic import-time trigger are both real
/// (`table_workspace::register_content_as_table`), but there is no
/// standalone "re-run Inspection on an already-registered Table"
/// orchestration anywhere — that sequence (load the Table's real content
/// via `ParquetContentStore`, call `NativeInspectionEngine::inspect`,
/// register the report) was never built as its own callable unit.
pub async fn inspect_table(_table_id: TableId) -> Result<InspectionReport, CommandError> {
    todo!()
}

/// GENUINE GAP — see this task's Output. `validate_standard`/
/// `ValidationTarget::Table` are real and tested at the engine level, but
/// no command-layer orchestration exists for a TABLE target (fetch
/// Table, load content, resolve ruleset version, call the engine,
/// register the report, append a Table Event Log entry) — only the
/// Dataset-target orchestration (`project_workspace::run_validation_on_dataset`)
/// was built.
pub async fn validate_table(
    _table_id: TableId,
    _ruleset_id: ValidationRulesetId,
) -> Result<ValidationReport, CommandError> {
    todo!()
}

/// GENUINE GAP — see this task's Output. Same shape as `validate_table`:
/// `analyse_standard`/`AnalysisTarget::Table` are real and tested, but no
/// command-layer orchestration exists for a TABLE target — only the
/// Dataset-target orchestration (`project_workspace::run_analysis_on_dataset`)
/// was built. INV-010/029: same Analysis engine as `analyse_python`, run
/// with `ComputeBackend::Native`.
pub async fn analyse_native(
    _table_id: TableId,
    _ruleset_id: AnalysisRulesetId,
) -> Result<AnalysisReport, CommandError> {
    todo!()
}

/// GENUINE GAP — see this task's Output. Same Table-target orchestration
/// gap as `analyse_native`, plus a second, independent gap:
/// `PythonAnalysisClient::call` (the sidecar dispatch itself) is also
/// `todo!()`, with zero concrete implementations anywhere in the crate.
/// INV-029: requires `ComputeBackend::Python { status: Ready }`; any other
/// status yields `AnalysisCommandOutcome::NotExecuted`, never an error
/// that hides the reason and never a partial report (Rule 9).
pub async fn analyse_python(
    _table_id: TableId,
    _ruleset_id: AnalysisRulesetId,
    _backend: ComputeBackend,
) -> Result<AnalysisCommandOutcome, CommandError> {
    todo!()
}

/// INV-013: Dataset targets only — there is no Table-accepting overload,
/// and none may ever be added. Wired to the real sequence this sweep
/// found already built and tested as `project_workspace::run_transformation`
/// (load real content, call `NativeTransformationEngine::transform`, call
/// `Dataset::apply_change`, append a `ProjectOperationLogEntry` — with no
/// exception for "trivial" operations) — a single operation is just a
/// one-element sequence to that function, not a second implementation.
/// See `commands::transform_flow_tests` for the sequence itself, proven
/// end to end against the real engine/type layer.
pub async fn transform_dataset(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    operation: TransformationOperation,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    project_workspace::run_transformation(dataset_repo, project_repo, report_repo, content_store, dataset_id, vec![operation], created_at, created_by).await
}

/// GENUINE GAP (not a wiring gap) — see this task's Output.
/// `TransformationEngine::transform_many` is real and tested at the
/// engine level (`transform_flow_tests::a_multi_dataset_operation_
/// produces_a_new_dataset_not_an_in_place_mutation`), but no command-
/// layer orchestration exists anywhere that fetches multiple Datasets,
/// calls it, and logs the result the way `run_transformation` does for
/// the single-Dataset case — that orchestration itself was never built,
/// not merely left unwired.
pub async fn transform_dataset_many(
    dataset_ids: Vec<DatasetId>,
    operation: TransformationOperation,
) -> Result<Dataset, CommandError> {
    let _ = (dataset_ids, operation);
    todo!()
}

/// Wired to `project_workspace::apply_recipe_to_dataset` — the thin part
/// this command adds is resolving `recipe_id` into the `RecipeContext`
/// that function expects (a specific, resolved version + its steps),
/// via the real, already-tested `KnowledgeRepository::get_recipe`.
#[allow(clippy::too_many_arguments)]
pub async fn apply_recipe(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    knowledge_repo: &impl KnowledgeRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    recipe_id: RecipeId,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    let recipe = knowledge_repo.get_recipe(recipe_id).await?.ok_or(CommandError::NotFound)?;
    let context = project_workspace::RecipeContext { recipe_id: recipe.id(), version: recipe.version(), steps: recipe.steps().to_vec() };
    project_workspace::apply_recipe_to_dataset(dataset_repo, project_repo, report_repo, content_store, dataset_id, context, created_at, created_by).await
}

/// GENUINE GAP (not a wiring gap) — see this task's Output. INV-007:
/// `source_operation_log_entry` must be a Project Operation Log entry.
/// `project_workspace::promote_operation_log_to_recipe` is real and
/// tested, and enforces exactly this invariant — but under an
/// INCOMPATIBLE signature: it takes `project_id` + a step range, not a
/// bare `LogEntryId`. Bridging the two would mean adding a new "which
/// project owns this log entry" reverse-lookup capability that exists
/// nowhere in this codebase (`ProjectRepository` has no such method) —
/// that is new logic, not delegation, so it is not done here.
pub async fn promote_recipe(_source_operation_log_entry: LogEntryId) -> Result<Recipe, CommandError> {
    todo!()
}

/// The starting point for a lineage query — either a Table or a Dataset,
/// per Part A/B of the Lineage Report task. No third variant: a Report
/// or a Project has never been requested as a lineage query object
/// anywhere in this project's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineageQueryObject {
    Table(TableId),
    Dataset(DatasetId),
}

/// Every `AnyReport` variant this codebase can actually construct today
/// (all six, as of the Audit Report task) shares the same baseline
/// `report_id`/`source_context` accessors. One small helper, not a
/// per-call-site match repeated six times. Always `Some` now — kept
/// returning `Option` rather than a bare tuple since callers
/// (`attach_report_edges`) still reasonably want a uniform "resolve or
/// skip" shape for whatever the next report kind turns out to need.
fn any_report_identity(report: &AnyReport) -> Option<(ReportId, SourceContext)> {
    match report {
        AnyReport::Inspection(r) => Some((r.report_id(), r.source_context().clone())),
        AnyReport::Validation(r) => Some((r.report_id(), r.source_context().clone())),
        AnyReport::Analysis(r) => Some((r.report_id(), r.source_context().clone())),
        AnyReport::Lineage(r) => Some((r.report_id(), r.source_context().clone())),
        AnyReport::Comparison(r) => Some((r.report_id(), r.source_context().clone())),
        AnyReport::Audit(r) => Some((r.report_id(), r.source_context().clone())),
    }
}

/// Part B's "report references" step: every Report whose `source_context`
/// genuinely points at `node`, attached as a `GeneratedReport` edge. Same
/// scan-and-filter shape `table_workspace::open_table_reports` already
/// uses (real precedent in this codebase for this exact tradeoff, not a
/// new one introduced here) — bounded by the number of reports that
/// exist, not by anything unbounded.
async fn attach_report_edges(report_repo: &impl ReportRepository, node: LineageNode, nodes: &mut Vec<LineageNode>, edges: &mut Vec<LineageEdge>) -> Result<(), CommandError> {
    // Deliberately excludes `ReportKind::Lineage` itself: a Lineage
    // Report describing the same node would otherwise become a new
    // `generated_report` edge the NEXT lineage generation for that node
    // picks up, which the one after that would pick up too — an
    // unbounded, self-referential graph growth that also breaks Part D's
    // idempotency (each call would produce a graph one edge bigger than
    // the last, purely from lineage generation being run before, not
    // from any real change to the underlying data). `Comparison` isn't
    // listed either, but harmlessly rather than deliberately: its
    // `source_context` is always a neutral placeholder (see
    // `ComparisonReport`'s own doc comment), so it could never match
    // `points_at_node` below even if scanned.
    //
    // `ReportKind::Audit` was added here by the Audit Report task — its
    // `source_context` is a real `project_dataset` reference (Part D of
    // that task), so an Audit Report genuinely does belong in a
    // Dataset's lineage graph, same as Inspection/Validation/Analysis.
    // This is the same "keep this scan's kind-list in sync with which
    // kinds are actually real" maintenance as `open_report`'s match arm
    // needed — not a revisit of the separate `RecipeApplied` exclusion
    // decision, which this task leaves untouched.
    for kind in [ReportKind::Inspection, ReportKind::Validation, ReportKind::Analysis, ReportKind::Audit] {
        for report in report_repo.list_by_kind(kind).await? {
            let Some((report_id, source_context)) = any_report_identity(&report) else { continue };
            let points_at_node = match node {
                LineageNode::ImportedTable(table_id) => source_context.table_id == Some(table_id),
                LineageNode::Dataset(dataset_id) => source_context.project_dataset_id == Some(dataset_id),
                LineageNode::Report(_) => false,
            };
            if points_at_node {
                edges.push(LineageEdge::GeneratedReport { source: node, report_id });
                let report_node = LineageNode::Report(report_id);
                if !nodes.contains(&report_node) {
                    nodes.push(report_node);
                }
            }
        }
    }
    Ok(())
}

/// Part B's real backward traversal — a direct, targeted walk of the
/// known reference fields, not a generic graph-traversal engine (the
/// task's own "must not" section). See this task's Output for the full
/// accounting of which of the product spec's named edge types this
/// traversal can and cannot honestly produce, and why.
///
/// Forward (downstream) traversal is explicitly out of scope — see
/// `LineageContent::downstream_count`'s own doc comment.
async fn build_lineage_content(
    table_repo: &impl TableRepository,
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    query: LineageQueryObject,
) -> Result<LineageContent, CommandError> {
    enum Item {
        Table(TableId),
        Dataset(DatasetId),
    }

    let mut nodes: Vec<LineageNode> = match query {
        LineageQueryObject::Table(id) => vec![LineageNode::ImportedTable(id)],
        LineageQueryObject::Dataset(id) => vec![LineageNode::Dataset(id)],
    };
    let mut edges: Vec<LineageEdge> = Vec::new();
    let mut archived_or_purged: Vec<LineageNode> = Vec::new();
    let mut upstream_count: u64 = 0;
    let mut reproducible = true;
    let mut visited_tables: std::collections::HashSet<TableId> = Default::default();
    let mut visited_datasets: std::collections::HashSet<DatasetId> = Default::default();

    let mut queue: Vec<Item> = match query {
        LineageQueryObject::Table(id) => vec![Item::Table(id)],
        LineageQueryObject::Dataset(id) => vec![Item::Dataset(id)],
    };

    while let Some(item) = queue.pop() {
        match item {
            Item::Table(table_id) => {
                if !visited_tables.insert(table_id) {
                    continue;
                }
                let Some(table) = table_repo.get(table_id).await? else {
                    // A reference points at a Table that no longer
                    // resolves — never silently drop this from
                    // `reproducible`'s verdict.
                    reproducible = false;
                    continue;
                };

                // Part C: stop traversing past an archived/purged node —
                // its own further references may not be trustworthy or
                // loadable. Only Tables have this concept in the current
                // data model (see `LineageContent::archived_or_purged`'s
                // own doc comment).
                let is_purged = table_repo.get_purged_ref(table_id).await?.is_some();
                if table.table_state() == TableState::Archived || is_purged {
                    archived_or_purged.push(LineageNode::ImportedTable(table_id));
                    reproducible = false;
                    continue;
                }

                // Backward from an exported Table.
                if table.creation_source().creation_source_type == CreationSourceType::ProjectDatasetExport {
                    match table.source_provenance().source_dataset_id {
                        Some(source_dataset_id) => {
                            edges.push(LineageEdge::ExportAsTable { dataset_id: source_dataset_id, table_id });
                            upstream_count += 1;
                            if !nodes.contains(&LineageNode::Dataset(source_dataset_id)) {
                                nodes.push(LineageNode::Dataset(source_dataset_id));
                            }
                            queue.push(Item::Dataset(source_dataset_id));
                        }
                        // The historical gap this task's own Part B asked
                        // to check: if this field is ever unpopulated on
                        // a real ProjectDatasetExport-sourced Table,
                        // never fabricate the edge — just mark the graph
                        // non-reproducible past this point.
                        None => reproducible = false,
                    }
                }

                attach_report_edges(report_repo, LineageNode::ImportedTable(table_id), &mut nodes, &mut edges).await?;
            }
            Item::Dataset(dataset_id) => {
                if !visited_datasets.insert(dataset_id) {
                    continue;
                }
                let Some(dataset) = dataset_repo.get(dataset_id).await? else {
                    reproducible = false;
                    continue;
                };

                if let Some(table_id) = dataset.source_table_id() {
                    edges.push(LineageEdge::LoadIntoProject { table_id, dataset_id });
                    upstream_count += 1;
                    if !nodes.contains(&LineageNode::ImportedTable(table_id)) {
                        nodes.push(LineageNode::ImportedTable(table_id));
                    }
                    queue.push(Item::Table(table_id));
                }

                // `Dataset.source_dataset_id`'s only real producer
                // anywhere in the command layer is `duplicate_dataset` —
                // see `LineageEdge::DatasetDuplicated`'s own doc comment
                // for why this isn't folded into `TransformationRun`.
                if let Some(parent_dataset_id) = dataset.source_dataset_id() {
                    edges.push(LineageEdge::DatasetDuplicated { from_dataset_id: parent_dataset_id, to_dataset_id: dataset_id });
                    upstream_count += 1;
                    if !nodes.contains(&LineageNode::Dataset(parent_dataset_id)) {
                        nodes.push(LineageNode::Dataset(parent_dataset_id));
                    }
                    queue.push(Item::Dataset(parent_dataset_id));
                }

                // Real transformation history: every Project Operation
                // Log entry on this Dataset that carries a
                // `transformation_operation` — `run_transformation`
                // mutates the Dataset in place (`Dataset::apply_change`),
                // so this is a genuine self-referencing edge, not a
                // fabricated second node.
                match project_repo.get(dataset.project_id()).await? {
                    Some(project) => {
                        for entry in project.operation_log().entries() {
                            if entry.dataset_id() == dataset_id && entry.transformation_operation().is_some() {
                                edges.push(LineageEdge::TransformationRun { dataset_id, operation_log_entry_id: entry.id(), operation: entry.operation().to_string() });
                                upstream_count += 1;
                            }
                        }
                    }
                    None => reproducible = false,
                }

                attach_report_edges(report_repo, LineageNode::Dataset(dataset_id), &mut nodes, &mut edges).await?;
            }
        }
    }

    Ok(LineageContent { nodes, edges, upstream_count, downstream_count: None, archived_or_purged, reproducible })
}

/// Real, end to end: `build_lineage_content` above queries already-real,
/// already-tracked references (never computes or infers anything —
/// Rule 8/INV-010, this is not a sixth processing engine); the result is
/// handed to `NativeReportEngine::generate_lineage_report`, the one
/// place permitted to construct the `LineageReport` object (INV-015);
/// registration goes through the same idempotent, state-machine-gated
/// `ReportRepository::register_lineage_report` flow every other report
/// kind uses (Part D). Idempotency key: the query object's identity plus
/// a hash of the resulting graph content (the report's own
/// `content_hash`, already computed from that exact content by the
/// engine) — regenerating lineage for unchanged underlying data hits the
/// idempotent path rather than duplicating, same discipline as
/// `import_table`'s own idempotency key.
pub async fn generate_lineage_report(
    table_repo: &impl TableRepository,
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    query: LineageQueryObject,
    created_at: Timestamp,
    created_by: String,
) -> Result<RegistrationOutcome, CommandError> {
    let content = build_lineage_content(table_repo, dataset_repo, project_repo, report_repo, query).await?;

    let (source_context, subject) = match query {
        LineageQueryObject::Table(id) => (SourceContext { source_context_type: SourceContextKind::Table, table_id: Some(id), project_dataset_id: None, project_id: None }, format!("table:{id}")),
        LineageQueryObject::Dataset(id) => (SourceContext { source_context_type: SourceContextKind::ProjectDataset, table_id: None, project_dataset_id: Some(id), project_id: None }, format!("dataset:{id}")),
    };

    let report_engine = NativeReportEngine;
    let generated = report_engine.generate_lineage_report(content, source_context, Uuid::new_v4(), created_by.clone(), created_at);

    let content_bytes = serde_json::to_vec(&generated.content).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    let idempotency_key = format!("{subject}:lineage_report:{}", generated.report.content_hash());

    let request = ReportRegistrationRequest {
        report_registration_request_id: Uuid::new_v4(),
        idempotency_key,
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
        log_refs: Vec::new(),
    };

    Ok(report_repo.register_lineage_report(request).await?)
}

/// Real, end to end: fetches both reports' real metadata and real
/// content bytes (the same `fetch_report_content` the Report Viewer
/// uses — one mechanism, not a second one for comparison specifically),
/// hands both to `NativeReportEngine::generate_comparison_report` — the
/// one place permitted to construct a `ComparisonReport` (INV-015) and
/// the one canonical choke point that enforces INV-016 (same-type only;
/// mismatched kinds yield
/// `CommandError::Report(ReportEngineError::MismatchedReportKinds)`) —
/// then registers through the same idempotent, state-machine-gated
/// `ReportRepository::register_comparison_report` flow every other
/// report kind uses (Part D). Idempotency key: both input report ids
/// (order-independent — comparing (A,B) and (B,A) hits the same key)
/// plus both their real `content_hash`es, so re-comparing an unchanged
/// pair returns the existing comparison rather than duplicating one.
pub async fn generate_comparison_report(
    report_repo: &impl ReportRepository,
    report_a_id: ReportId,
    report_b_id: ReportId,
    created_at: Timestamp,
    created_by: String,
) -> Result<RegistrationOutcome, CommandError> {
    // Order-independence, for real: the diff itself is asymmetric
    // (comparing A->B swaps Added/Removed relative to B->A), so
    // "comparing (A,B) and (B,A) is the same comparison" requires
    // normalizing WHICH report plays "a" and which plays "b" before any
    // fetching/diffing happens — sorting only the idempotency key
    // string while still diffing in caller-supplied order was found,
    // during this task's own testing, to still produce two genuinely
    // different `ComparisonContent` values (and so two different
    // registrations) for the same pair.
    let (report_a_id, report_b_id) = if report_a_id <= report_b_id { (report_a_id, report_b_id) } else { (report_b_id, report_a_id) };

    let report_a = report_repo.get(report_a_id).await?.ok_or(CommandError::NotFound)?;
    let report_b = report_repo.get(report_b_id).await?.ok_or(CommandError::NotFound)?;
    let content_a = fetch_report_content(report_repo, &report_a).await?;
    let content_b = fetch_report_content(report_repo, &report_b).await?;

    let report_engine = NativeReportEngine;
    let generated = report_engine.generate_comparison_report(&report_a, &report_b, content_a, content_b, Uuid::new_v4(), created_by.clone(), created_at)?;

    let content_bytes = serde_json::to_vec(&generated.content).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    // Already normalized above, so this alone is order-independent.
    let idempotency_key = format!("{report_a_id}:{report_b_id}:comparison_report:{}", generated.report.content_hash());

    let request = ReportRegistrationRequest {
        report_registration_request_id: Uuid::new_v4(),
        idempotency_key,
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
        log_refs: Vec::new(),
    };

    Ok(report_repo.register_comparison_report(request, generated.report.compared_report_a_id(), generated.report.compared_report_b_id()).await?)
}

/// Produces a `PurgedRef` content tombstone; never removes the Table's
/// identity/metadata (see PurgedRef, INV-002). Wired to the real
/// `TableRepository::purge`/`get_purged_ref` primitives (implemented,
/// though previously never called from any command) — this function is
/// the thin assembly this sweep's Method calls for: fetch the Table for
/// its real fingerprint, construct the `PurgedRef` its own documented
/// contract already fully specifies, record it.
pub async fn purge_table(table_repo: &impl TableRepository, table_id: TableId, reason: String, purged_at: Timestamp) -> Result<crate::types::PurgedRef, CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    let purged_ref = crate::types::PurgedRef::new(table_id, table.table_data_ref().data_fingerprint.clone(), purged_at, reason);
    table_repo.purge(purged_ref.clone()).await?;
    Ok(purged_ref)
}

/// Wired to the real implementation this sweep found sitting unwired:
/// `project_workspace::create_project` (built, and tested, in the
/// Project Workspace backend task) — the second confirmed instance of
/// the `import_table`-shaped pattern this task exists to sweep for.
pub async fn create_project(project_repo: &impl ProjectRepository, name: String, created_at: Timestamp) -> Result<Project, CommandError> {
    Ok(project_workspace::create_project(project_repo, name, created_at).await?)
}

/// GENUINE GAP (not a wiring gap) — see this task's Output. No real
/// implementation exists anywhere for the `source_table_id: None` case
/// (a Dataset created with no ancestor at all); `project_workspace::
/// load_table_as_dataset` only covers the `Some(table_id)` case, and
/// wiring only that half would silently succeed for one branch and
/// `todo!()` the other — the same "looks wired, isn't really" shape this
/// whole line of tasks exists to catch, not something to introduce here.
pub async fn create_dataset(
    _project_id: ProjectId,
    _source_table_id: Option<TableId>,
    _name: String,
) -> Result<Dataset, CommandError> {
    todo!()
}

/// Project Workspace UI wiring task, Part A: `open_project`,
/// `list_projects`, `load_table_as_dataset`, `duplicate_dataset`,
/// `export_dataset_as_table`, `save_pending_plan`/`load_pending_plan`/
/// `run_pending_plan`, `load_recipe_into_project`,
/// `run_validation_on_dataset`, `run_analysis_on_dataset` had no
/// `commands.rs` presence at all before this task — not `todo!()`, just
/// absent — so the sweep's "wire the todo!()" shape doesn't literally
/// apply, but the underlying finding is the same: a real, already-tested
/// `project_workspace::*` implementation existed with no canonical
/// command-layer entry point (INV-023) for the UI to call. Each function
/// below is exactly the one-line delegation shape the sweep already
/// established for `create_project`/`transform_dataset`/`apply_recipe` —
/// no added logic, no new decision, just the missing canonical entry
/// point.
pub async fn open_project(project_repo: &impl ProjectRepository, project_id: ProjectId) -> Result<Project, CommandError> {
    project_workspace::open_project(project_repo, project_id).await
}

pub async fn list_projects(project_repo: &impl ProjectRepository) -> Result<Vec<project_workspace::ProjectSummary>, CommandError> {
    project_workspace::list_projects(project_repo).await
}

#[allow(clippy::too_many_arguments)]
pub async fn load_table_as_dataset(
    table_repo: &impl TableRepository,
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    project_id: ProjectId,
    table_id: TableId,
    name: String,
    created_at: Timestamp,
) -> Result<Dataset, CommandError> {
    project_workspace::load_table_as_dataset(table_repo, dataset_repo, project_repo, project_id, table_id, name, created_at).await
}

pub async fn duplicate_dataset(dataset_repo: &impl DatasetRepository, project_repo: &impl ProjectRepository, dataset_id: DatasetId, name: String, created_at: Timestamp) -> Result<Dataset, CommandError> {
    project_workspace::duplicate_dataset(dataset_repo, project_repo, dataset_id, name, created_at).await
}

#[allow(clippy::too_many_arguments)]
pub async fn export_dataset_as_table(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    table_repo: &impl TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    table_name: String,
    created_at: Timestamp,
    created_by: String,
) -> Result<Table, CommandError> {
    project_workspace::export_dataset_as_table(dataset_repo, project_repo, table_repo, report_repo, content_store, dataset_id, table_name, created_at, created_by).await
}

pub async fn save_pending_plan(
    plan_repo: &impl PendingPlanRepository,
    project_id: ProjectId,
    dataset_id: DatasetId,
    name: String,
    operations: Vec<TransformationOperation>,
    created_at: Timestamp,
) -> Result<PendingTransformationPlan, CommandError> {
    project_workspace::save_pending_plan(plan_repo, project_id, dataset_id, name, operations, created_at).await
}

pub async fn load_pending_plan(plan_repo: &impl PendingPlanRepository, plan_id: Uuid) -> Result<PendingTransformationPlan, CommandError> {
    project_workspace::load_pending_plan(plan_repo, plan_id).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_pending_plan(
    plan_repo: &impl PendingPlanRepository,
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    plan_id: Uuid,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    project_workspace::run_pending_plan(plan_repo, dataset_repo, project_repo, report_repo, content_store, plan_id, created_at, created_by).await
}

pub async fn load_recipe_into_project(knowledge_repo: &impl KnowledgeRepository, recipe_id: Uuid, version: u32) -> Result<project_workspace::RecipeContext, CommandError> {
    project_workspace::load_recipe_into_project(knowledge_repo, recipe_id, version).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_validation_on_dataset(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    knowledge_repo: &impl KnowledgeRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    detail_store: &DetailTableStore,
    dataset_id: DatasetId,
    ruleset_id: ValidationRulesetId,
    ruleset_version_id: Uuid,
    created_at: Timestamp,
    created_by: String,
) -> Result<RegistrationOutcome, CommandError> {
    project_workspace::run_validation_on_dataset(dataset_repo, project_repo, knowledge_repo, report_repo, content_store, detail_store, dataset_id, ruleset_id, ruleset_version_id, created_at, created_by).await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_analysis_on_dataset(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    knowledge_repo: &impl KnowledgeRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    detail_store: &DetailTableStore,
    dataset_id: DatasetId,
    ruleset_id: AnalysisRulesetId,
    ruleset_version_id: Uuid,
    created_at: Timestamp,
    created_by: String,
) -> Result<RegistrationOutcome, CommandError> {
    project_workspace::run_analysis_on_dataset(dataset_repo, project_repo, knowledge_repo, report_repo, content_store, detail_store, dataset_id, ruleset_id, ruleset_version_id, created_at, created_by).await
}
