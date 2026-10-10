//! Thin Tauri command wrappers. INV-022: UI components invoke these, never
//! engines or repositories directly. Each wrapper does nothing but forward
//! to its canonical `nikti_core::commands` function (INV-023) and map the
//! error to a string for the IPC boundary — no business logic lives here.
//!
//! The Table Repository Workspace wrappers below additionally extract a
//! `tauri::State<..>` handle to a repository and pass it straight through
//! — that state extraction is DI plumbing, not business logic; every
//! decision (state-action matrix, idempotency, dictionary gating, ...)
//! still lives in `nikti_core::commands`/`nikti_core::commands::table_workspace`.
//! The concrete repositories managed here are the same `testing::InMemory*`
//! reference implementations already proven correct by
//! `nikti-core`'s own test suite — NOT the frozen embedded-relational-
//! store tech decision, which is separate, later work (same status this
//! whole codebase has consistently given real persistence).

use std::time::{SystemTime, UNIX_EPOCH};

use nikti_core::commands;
use nikti_core::repositories::dataset_repository::testing::InMemoryDatasetRepository;
use nikti_core::repositories::knowledge_repository::testing::InMemoryKnowledgeRepository;
use nikti_core::repositories::pending_plan_repository::testing::InMemoryPendingPlanRepository;
use nikti_core::repositories::project_repository::testing::InMemoryProjectRepository;
use nikti_core::repositories::report_repository::testing::InMemoryReportRepository;
use nikti_core::repositories::table_repository::testing::InMemoryTableRepository;
use nikti_core::repositories::{AnyReport, DetailTableStore, ParquetContentStore};
use nikti_core::sidecar::ComputeBackend;
use nikti_core::types::common::{
    AnalysisRulesetId, DatasetId, LogEntryId, ProjectId, RecipeId, ReportId, TableId,
    ValidationRulesetId,
};
use nikti_core::types::{
    AnalysisReport, Dataset, InspectionReport, PendingTransformationPlan, Project, Recipe,
    RegistrationOutcome, Table, TableEventLog, TransformationOperation, ValidationReport,
};

/// Part H: "Only if policy allows" for opening an Archived Table
/// read-only. A fixed application-level policy, decided here (the one
/// place wall-clock/environment-level configuration is allowed to enter
/// the system) — never a per-call choice the frontend makes, which would
/// itself be a business decision living on the wrong side of INV-022.
const ARCHIVED_OPEN_POLICY: bool = true;

/// No auth/user-account system exists yet — this is the one place a
/// placeholder "current user" enters the system, same status as
/// `ARCHIVED_OPEN_POLICY`.
const CURRENT_USER: &str = "nikti-app";

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before UNIX_EPOCH").as_millis() as i64
}

#[tauri::command]
pub async fn import_table(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    uri: String,
) -> Result<Table, String> {
    commands::import_table(table_repo.inner(), report_repo.inner(), content_store.inner(), commands::ImportSource { uri }, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table(table_id: TableId) -> Result<Table, String> {
    commands::open_table(table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn inspect_table(table_id: TableId) -> Result<InspectionReport, String> {
    commands::inspect_table(table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_table(
    table_id: TableId,
    ruleset_id: ValidationRulesetId,
) -> Result<ValidationReport, String> {
    commands::validate_table(table_id, ruleset_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn analyse_native(
    table_id: TableId,
    ruleset_id: AnalysisRulesetId,
) -> Result<AnalysisReport, String> {
    commands::analyse_native(table_id, ruleset_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn analyse_python(
    table_id: TableId,
    ruleset_id: AnalysisRulesetId,
    backend: ComputeBackend,
) -> Result<AnalysisReport, String> {
    match commands::analyse_python(table_id, ruleset_id, backend)
        .await
        .map_err(|e| e.to_string())?
    {
        commands::AnalysisCommandOutcome::Completed(report) => Ok(report),
        commands::AnalysisCommandOutcome::NotExecuted { reason } => Err(reason),
    }
}

#[tauri::command]
pub async fn transform_dataset(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    dataset_id: DatasetId,
    operation: TransformationOperation,
) -> Result<Dataset, String> {
    commands::transform_dataset(dataset_repo.inner(), project_repo.inner(), report_repo.inner(), content_store.inner(), dataset_id, operation, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transform_dataset_many(
    dataset_ids: Vec<DatasetId>,
    operation: TransformationOperation,
) -> Result<Dataset, String> {
    commands::transform_dataset_many(dataset_ids, operation).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_recipe(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    knowledge_repo: tauri::State<'_, InMemoryKnowledgeRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    dataset_id: DatasetId,
    recipe_id: RecipeId,
) -> Result<Dataset, String> {
    commands::apply_recipe(dataset_repo.inner(), project_repo.inner(), knowledge_repo.inner(), report_repo.inner(), content_store.inner(), dataset_id, recipe_id, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn promote_recipe(source_operation_log_entry: LogEntryId) -> Result<Recipe, String> {
    commands::promote_recipe(source_operation_log_entry)
        .await
        .map_err(|e| e.to_string())
}

// Exactly one of `table_id`/`dataset_id` must be provided — mirrors
// `commands::LineageQueryObject`'s two real variants. Plain `Option`
// params rather than a tagged-union param because Tauri command args
// are a flat JSON object; the command layer is still what owns the
// actual query-object type and its exhaustive-match guarantee.
#[tauri::command]
pub async fn generate_lineage_report(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    table_id: Option<TableId>,
    dataset_id: Option<DatasetId>,
) -> Result<RegistrationOutcome, String> {
    let query = match (table_id, dataset_id) {
        (Some(t), None) => commands::LineageQueryObject::Table(t),
        (None, Some(d)) => commands::LineageQueryObject::Dataset(d),
        _ => return Err("exactly one of table_id/dataset_id must be provided".to_string()),
    };
    commands::generate_lineage_report(table_repo.inner(), dataset_repo.inner(), project_repo.inner(), report_repo.inner(), query, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn generate_comparison_report(report_repo: tauri::State<'_, InMemoryReportRepository>, a: ReportId, b: ReportId) -> Result<RegistrationOutcome, String> {
    commands::generate_comparison_report(report_repo.inner(), a, b, now_ms(), CURRENT_USER.to_string()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn purge_table(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId, reason: String) -> Result<nikti_core::types::PurgedRef, String> {
    commands::purge_table(table_repo.inner(), table_id, reason, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_project(project_repo: tauri::State<'_, InMemoryProjectRepository>, name: String) -> Result<Project, String> {
    commands::create_project(project_repo.inner(), name, now_ms()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_dataset(
    project_id: ProjectId,
    source_table_id: Option<TableId>,
    name: String,
) -> Result<Dataset, String> {
    commands::create_dataset(project_id, source_table_id, name)
        .await
        .map_err(|e| e.to_string())
}

// --- Table Repository Workspace (v2) ---
// `import_table`/`create_dataset`/`open_table`/`inspect_table` above are
// shared with the older, generic command set; the wrappers below are the
// remaining 17 of the 19 canonical `table.*` commands. `table.import` is
// the existing `import_table` wrapper (now real, real-pipeline task —
// updated above to take repository/content-store state);
// `table.reimport_as_new` is the one below, also real.

#[tauri::command]
pub async fn reimport_table_as_new(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    original_table_id: TableId,
    uri: String,
) -> Result<Table, String> {
    commands::reimport_table_as_new(table_repo.inner(), report_repo.inner(), content_store.inner(), original_table_id, commands::ImportSource { uri }, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_tables(table_repo: tauri::State<'_, InMemoryTableRepository>, query: String) -> Result<Vec<Table>, String> {
    commands::search_tables(table_repo.inner(), query).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_table_search(table_repo: tauri::State<'_, InMemoryTableRepository>) -> Result<Vec<Table>, String> {
    commands::clear_table_search(table_repo.inner()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_table_repository(table_repo: tauri::State<'_, InMemoryTableRepository>) -> Result<Vec<Table>, String> {
    commands::refresh_table_repository(table_repo.inner()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table_readonly(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<Table, String> {
    commands::open_table_readonly(table_repo.inner(), table_id, ARCHIVED_OPEN_POLICY)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table_information(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<Table, String> {
    commands::open_table_information(table_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table_dictionary(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    table_id: TableId,
) -> Result<commands::table_workspace::DictionaryNavigation, String> {
    commands::open_table_dictionary(table_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table_event_log(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<TableEventLog, String> {
    commands::open_table_event_log(table_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_table_reports(report_repo: tauri::State<'_, InMemoryReportRepository>, table_id: TableId) -> Result<Vec<AnyReport>, String> {
    commands::open_table_reports(report_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

// The Report Viewer's one canonical command — called from Table
// Repository's Reports tab, Project Workspace's Dataset Reports tab, and
// any future Report Repository workspace alike (INV-022/023).
#[tauri::command]
pub async fn open_report(report_repo: tauri::State<'_, InMemoryReportRepository>, report_id: ReportId) -> Result<commands::ReportWithContent, String> {
    commands::open_report(report_repo.inner(), report_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_table_into_project(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    table_id: TableId,
    project_id: ProjectId,
) -> Result<Dataset, String> {
    commands::load_table_into_project(table_repo.inner(), project_repo.inner(), dataset_repo.inner(), table_id, project_id, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_project_from_table(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    table_id: TableId,
    new_project_name: String,
) -> Result<(Project, Dataset), String> {
    commands::create_project_from_table_command(table_repo.inner(), project_repo.inner(), dataset_repo.inner(), table_id, new_project_name, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn route_table_inspection(table_id: TableId) -> Result<InspectionReport, String> {
    commands::route_table_inspection(table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn route_table_validation(
    table_id: TableId,
    ruleset_version_id: Option<ValidationRulesetId>,
) -> Result<commands::table_workspace::RulesetManagerNavigation, String> {
    commands::route_table_validation(table_id, ruleset_version_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn route_table_analysis(
    table_id: TableId,
    ruleset_version_id: Option<AnalysisRulesetId>,
) -> Result<commands::table_workspace::RulesetManagerNavigation, String> {
    commands::route_table_analysis(table_id, ruleset_version_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn archive_table(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<Table, String> {
    commands::archive_table(table_repo.inner(), table_id, now_ms()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_table(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<Table, String> {
    commands::restore_table(table_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn retry_table_availability_check(table_repo: tauri::State<'_, InMemoryTableRepository>, table_id: TableId) -> Result<Table, String> {
    commands::retry_table_availability_check(table_repo.inner(), table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn copy_table_id(table_id: TableId) -> Result<String, String> {
    commands::copy_table_id(table_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn table_allowed_actions(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    table_id: TableId,
) -> Result<commands::table_workspace::AllowedActions, String> {
    commands::table_allowed_actions(table_repo.inner(), table_id, ARCHIVED_OPEN_POLICY)
        .await
        .map_err(|e| e.to_string())
}

// --- Project Workspace UI wiring ---
// Same DI/error-mapping shape as every wrapper above — no business logic,
// only `tauri::State` extraction and `.map_err(|e| e.to_string())`. Every
// decision (dataset construction, log-append discipline, Recipe version
// resolution, ...) lives in `nikti_core::commands`/`project_workspace`.

#[tauri::command]
pub async fn open_project(project_repo: tauri::State<'_, InMemoryProjectRepository>, project_id: ProjectId) -> Result<Project, String> {
    commands::open_project(project_repo.inner(), project_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_projects(project_repo: tauri::State<'_, InMemoryProjectRepository>) -> Result<Vec<commands::project_workspace::ProjectSummary>, String> {
    commands::list_projects(project_repo.inner()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_table_as_dataset(
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    project_id: ProjectId,
    table_id: TableId,
    name: String,
) -> Result<Dataset, String> {
    commands::load_table_as_dataset(table_repo.inner(), dataset_repo.inner(), project_repo.inner(), project_id, table_id, name, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duplicate_dataset(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    dataset_id: DatasetId,
    name: String,
) -> Result<Dataset, String> {
    commands::duplicate_dataset(dataset_repo.inner(), project_repo.inner(), dataset_id, name, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_dataset_as_table(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    table_repo: tauri::State<'_, InMemoryTableRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    dataset_id: DatasetId,
    table_name: String,
) -> Result<Table, String> {
    commands::export_dataset_as_table(dataset_repo.inner(), project_repo.inner(), table_repo.inner(), report_repo.inner(), content_store.inner(), dataset_id, table_name, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_pending_plan(
    plan_repo: tauri::State<'_, InMemoryPendingPlanRepository>,
    project_id: ProjectId,
    dataset_id: DatasetId,
    name: String,
    operations: Vec<TransformationOperation>,
) -> Result<PendingTransformationPlan, String> {
    commands::save_pending_plan(plan_repo.inner(), project_id, dataset_id, name, operations, now_ms())
        .await
        .map_err(|e| e.to_string())
}

// `plan_id`/`ruleset_version_id` below are bare `Uuid`s, not references to
// log entries — `LogEntryId` (itself a plain `Uuid` alias) is reused only
// because it's already imported here; there is no `PendingPlanId`/
// `RulesetVersionId` alias in `nikti_core::types::common` to import instead.
#[tauri::command]
pub async fn load_pending_plan(plan_repo: tauri::State<'_, InMemoryPendingPlanRepository>, plan_id: LogEntryId) -> Result<PendingTransformationPlan, String> {
    commands::load_pending_plan(plan_repo.inner(), plan_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_pending_plan(
    plan_repo: tauri::State<'_, InMemoryPendingPlanRepository>,
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    plan_id: LogEntryId,
) -> Result<Dataset, String> {
    commands::run_pending_plan(plan_repo.inner(), dataset_repo.inner(), project_repo.inner(), report_repo.inner(), content_store.inner(), plan_id, now_ms(), CURRENT_USER.to_string())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_recipe_into_project(
    knowledge_repo: tauri::State<'_, InMemoryKnowledgeRepository>,
    recipe_id: RecipeId,
    version: u32,
) -> Result<commands::project_workspace::RecipeContext, String> {
    commands::load_recipe_into_project(knowledge_repo.inner(), recipe_id, version)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_validation_on_dataset(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    knowledge_repo: tauri::State<'_, InMemoryKnowledgeRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    detail_store: tauri::State<'_, DetailTableStore>,
    dataset_id: DatasetId,
    ruleset_id: ValidationRulesetId,
    ruleset_version_id: LogEntryId,
) -> Result<RegistrationOutcome, String> {
    commands::run_validation_on_dataset(
        dataset_repo.inner(),
        project_repo.inner(),
        knowledge_repo.inner(),
        report_repo.inner(),
        content_store.inner(),
        detail_store.inner(),
        dataset_id,
        ruleset_id,
        ruleset_version_id,
        now_ms(),
        CURRENT_USER.to_string(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_analysis_on_dataset(
    dataset_repo: tauri::State<'_, InMemoryDatasetRepository>,
    project_repo: tauri::State<'_, InMemoryProjectRepository>,
    knowledge_repo: tauri::State<'_, InMemoryKnowledgeRepository>,
    report_repo: tauri::State<'_, InMemoryReportRepository>,
    content_store: tauri::State<'_, ParquetContentStore>,
    detail_store: tauri::State<'_, DetailTableStore>,
    dataset_id: DatasetId,
    ruleset_id: AnalysisRulesetId,
    ruleset_version_id: LogEntryId,
) -> Result<RegistrationOutcome, String> {
    commands::run_analysis_on_dataset(
        dataset_repo.inner(),
        project_repo.inner(),
        knowledge_repo.inner(),
        report_repo.inner(),
        content_store.inner(),
        detail_store.inner(),
        dataset_id,
        ruleset_id,
        ruleset_version_id,
        now_ms(),
        CURRENT_USER.to_string(),
    )
    .await
    .map_err(|e| e.to_string())
}
