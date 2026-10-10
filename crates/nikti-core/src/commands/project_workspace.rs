//! Project Workspace command-layer support (backend only — no React/Tauri
//! UI, per this task's own header; a separate UI-wiring task follows,
//! same split as the Table Repository). INV-022/023: every UI entry point
//! this workspace will eventually expose maps to exactly one of the
//! functions below, proven here against in-memory repositories.
//!
//! Part D (structural, not just runtime-checked): every function that
//! transforms/applies-a-recipe-to/validates/analyses working content
//! below takes a `DatasetId`, never a `TableId` — accepting a Table as a
//! transformation target must not typecheck, matching the same
//! discipline `TransformationEngine::transform` already enforces one
//! layer down (INV-002/013).

use std::ops::Range;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use polars::prelude::IntoLazy;

use crate::commands::table_workspace;
use crate::engines::analysis::{analyse_standard, AnalysisTarget};
use crate::engines::content::DatasetContent;
use crate::engines::report::{NativeReportEngine, ReportEngine};
use crate::engines::validation::{validate_standard, ValidationTarget};
use crate::engines::{NativeTransformationEngine, TransformationEngine};
use crate::repositories::table_import::{data_fingerprint, DetailTableStore, ParquetContentStore};
use crate::repositories::{DatasetRepository, KnowledgeRepository, PendingPlanRepository, ProjectRepository, ReportRepository};
use crate::types::common::{AnalysisRulesetId, DatasetId, ProjectId, Timestamp, ValidationRulesetId};
use crate::types::report_registration::SourceContext;
use crate::types::{
    CreationSource, CreationSourceType, Dataset, DefinitionStatus, PendingTransformationPlan, Project, ProjectOperationLogEntry, Recipe,
    RegistrationOutcome, ReportRegistrationRequest, SourceProvenance, SourceType, TransformationOperation, VersionState,
};
use crate::types::AnalysisVersionState;

use super::errors::CommandError;

/// `list_projects` — a flattened summary, never a second competing
/// object model (same Part C discipline as the Table Repository's
/// Navigator views): every field here is derived from the canonical
/// `Project`, not stored independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_id: ProjectId,
    pub name: String,
    pub dataset_count: usize,
    pub created_at: Timestamp,
}

/// `load_recipe_into_project` — a resolved, specific Recipe version ready
/// to apply, never "the latest" (same discipline
/// `ValidationRulesetVersion`/`AnalysisRulesetVersion` already establish
/// for their rulesets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeContext {
    pub recipe_id: Uuid,
    pub version: u32,
    pub steps: Vec<TransformationOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectWorkspaceError {
    #[error("recipe version mismatch: requested {requested}, recipe is at version {actual}")]
    RecipeVersionMismatch { requested: u32, actual: u32 },
    #[error("step_range is empty or out of bounds for this Project Operation Log")]
    InvalidStepRange,
    #[error("no Transformation step exists in the given range to promote into a Recipe")]
    NoTransformationStepsInRange,
}

impl From<ProjectWorkspaceError> for CommandError {
    fn from(err: ProjectWorkspaceError) -> Self {
        CommandError::Repository(crate::repositories::RepositoryError::Backend(err.to_string()))
    }
}

pub async fn create_project(project_repo: &impl ProjectRepository, name: String, created_at: Timestamp) -> Result<Project, CommandError> {
    let project = Project::new(Uuid::new_v4(), name, created_at);
    project_repo.insert(project.clone()).await?;
    Ok(project)
}

pub async fn open_project(project_repo: &impl ProjectRepository, project_id: ProjectId) -> Result<Project, CommandError> {
    project_repo.get(project_id).await?.ok_or(CommandError::NotFound)
}

pub async fn list_projects(project_repo: &impl ProjectRepository) -> Result<Vec<ProjectSummary>, CommandError> {
    let projects = project_repo.list().await?;
    Ok(projects
        .into_iter()
        .map(|p| ProjectSummary { project_id: p.id(), name: p.name().to_string(), dataset_count: p.dataset_ids().len(), created_at: p.created_at() })
        .collect())
}

/// The real implementation behind `table.load_into_project`/
/// `table.create_project_from_table` from the Table Repository task —
/// those commands construct the Dataset *value* already (`table_workspace::
/// load_into_project`); this is where it's actually persisted, given its
/// own independent identity, and logged. Never modifies the source Table
/// (INV-002/003): only ever reads it.
pub async fn load_table_as_dataset(
    table_repo: &impl crate::repositories::TableRepository,
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    project_id: ProjectId,
    table_id: crate::types::common::TableId,
    name: String,
    created_at: Timestamp,
) -> Result<Dataset, CommandError> {
    let table = table_repo.get(table_id).await?.ok_or(CommandError::NotFound)?;
    let mut project = project_repo.get(project_id).await?.ok_or(CommandError::NotFound)?;

    let dataset = Dataset::new(
        Uuid::new_v4(),
        project_id,
        name,
        Some(table.id()),
        None,
        table.schema().clone(),
        table.row_count(),
        table.table_data_ref().content_ref.clone(),
        table.table_data_ref().data_fingerprint.clone(),
        created_at,
    );
    dataset_repo.insert(dataset.clone()).await?;

    project.add_dataset(dataset.id());
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), project_id, dataset.id(), created_at, "load".to_string()));
    project_repo.save(project).await?;

    Ok(dataset)
}

pub async fn duplicate_dataset(dataset_repo: &impl DatasetRepository, project_repo: &impl ProjectRepository, dataset_id: DatasetId, name: String, created_at: Timestamp) -> Result<Dataset, CommandError> {
    let source = dataset_repo.get(dataset_id).await?.ok_or(CommandError::NotFound)?;
    let mut project = project_repo.get(source.project_id()).await?.ok_or(CommandError::NotFound)?;

    let duplicate = Dataset::new(
        Uuid::new_v4(),
        source.project_id(),
        name,
        source.source_table_id(),
        Some(source.id()),
        source.schema().clone(),
        source.row_count(),
        source.content_ref().to_string(),
        source.content_fingerprint().to_string(),
        created_at,
    );
    dataset_repo.insert(duplicate.clone()).await?;

    project.add_dataset(duplicate.id());
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), source.project_id(), duplicate.id(), created_at, "duplicate".to_string()));
    project_repo.save(project).await?;

    Ok(duplicate)
}

/// Calls into the same real registration path the import-pipeline task
/// built (`table_workspace::register_content_as_table`) — same content-
/// addressed Parquet + fingerprinting + idempotent registration, never a
/// shortcut. `creation_source_type = ProjectDatasetExport` (Amendment 4)
/// — never `Import`, so this can never be confused with a CSV-sourced
/// Table at the object-model level. Also appends the Project Operation
/// Log's own "export" entry (Part C) — distinct from the Table Event Log
/// entry `register_content_as_table` appends for the new Table itself;
/// two logs, two entries, one export.
pub async fn export_dataset_as_table(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    table_repo: &impl crate::repositories::TableRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    table_name: String,
    created_at: Timestamp,
    created_by: String,
) -> Result<crate::types::Table, CommandError> {
    let dataset = dataset_repo.get(dataset_id).await?.ok_or(CommandError::NotFound)?;
    let mut project = project_repo.get(dataset.project_id()).await?.ok_or(CommandError::NotFound)?;

    let frame = content_store.read(dataset.content_ref()).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;

    let table = table_workspace::register_content_as_table(
        table_repo,
        report_repo,
        content_store,
        frame,
        CreationSource { creation_source_type: CreationSourceType::ProjectDatasetExport, source_type: SourceType::ProjectDataset },
        SourceProvenance { source_label: dataset.name().to_string(), source_project_id: Some(dataset.project_id()), source_dataset_id: Some(dataset.id()) },
        table_name,
        format!("export:{dataset_id}:{}", Uuid::new_v4()),
        Uuid::new_v4(),
        Uuid::new_v4(),
        created_at,
        created_by,
        0,
    )
    .await?;

    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), dataset.project_id(), dataset_id, created_at, format!("export_as_table:{}", table.id())));
    project_repo.save(project).await?;

    Ok(table)
}

/// Shared by `run_transformation` (manual, `recipe_ref: None`) and
/// `apply_recipe_to_dataset` (`recipe_ref: Some((recipe_id, version))`)
/// — one implementation, not two, matching every other "two public
/// entry points, one real path" pattern already in this module
/// (`run_pending_plan` -> `run_transformation`, for instance).
///
/// Part C ordering, stated explicitly rather than only asserted: the
/// transformation's own commit — `dataset_repo.save`/`project_repo.save`,
/// which is what makes every step's Project Operation Log entry durable
/// — happens BEFORE `generate_and_register_audit_report` is even called.
/// This function's `Ok(dataset)` return does not depend on that call's
/// outcome at all: its result is deliberately discarded (`let _ =`), so
/// an Audit Report failure can never turn an already-committed,
/// already-logged transformation into an error return, a rollback, or a
/// retry of the transformation itself — degraded-but-recoverable, same
/// discipline as Inspection's import-time auto-trigger.
async fn run_transformation_steps(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    operations: Vec<TransformationOperation>,
    recipe_ref: Option<(Uuid, u32)>,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    let mut dataset = dataset_repo.get(dataset_id).await?.ok_or(CommandError::NotFound)?;
    let mut project = project_repo.get(dataset.project_id()).await?.ok_or(CommandError::NotFound)?;
    let engine = NativeTransformationEngine;

    // Part A/B: the real input state, captured once, before any
    // operation runs — the Audit Report's before-counts/fingerprint are
    // these exact values, never recomputed a second way later.
    let input_dataset_name = dataset.name().to_string();
    let input_row_count = dataset.row_count();
    let input_column_count = dataset.schema().columns.len();
    let input_frame = content_store.read(dataset.content_ref()).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    let input_schema_fingerprint = crate::repositories::table_import::schema_fingerprint(&input_frame.schema());

    let mut audit_steps: Vec<crate::types::AuditStep> = Vec::new();
    let mut operation_log_entry_ids: Vec<Uuid> = Vec::new();

    for operation in operations {
        let frame = content_store.read(dataset.content_ref()).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
        let content = DatasetContent { dataset_id, frame: frame.lazy() };
        let row_count_before = dataset.row_count();
        let column_count_before = dataset.schema().columns.len();
        let mut results = engine.transform(content, operation.clone()).map_err(|e| CommandError::Transform(e))?;

        let new_fingerprint = data_fingerprint(&results.frame).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
        let content_ref = content_store.write_if_absent(&new_fingerprint, &mut results.frame).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;

        let row_count_after = results.row_count();
        let column_count_after = results.schema().columns.len();
        let label = format!("{operation:?}");
        dataset.apply_change(content_ref, new_fingerprint, results.schema(), results.row_count(), label.clone(), None, created_at);

        // Rule 6/INV-020: appended for every single operation, regardless
        // of how small/fast it was — no batching exception.
        let entry_id = Uuid::new_v4();
        let entry = match recipe_ref {
            // Part A's root-cause fix: the one real place a Recipe-driven
            // step's `recipe_id`/`recipe_version` is actually recorded.
            Some((recipe_id, recipe_version)) => {
                ProjectOperationLogEntry::new_recipe_transformation(entry_id, dataset.project_id(), dataset_id, created_at, label, operation.clone(), recipe_id, recipe_version)
            }
            None => ProjectOperationLogEntry::new_transformation(entry_id, dataset.project_id(), dataset_id, created_at, label, operation.clone()),
        };
        project.operation_log_mut().append(entry);
        operation_log_entry_ids.push(entry_id);

        audit_steps.push(crate::types::AuditStep { operation, operation_log_entry_id: entry_id, row_count_before, row_count_after, column_count_before, column_count_after });
    }

    // The real commit — this line is what makes the transformation and
    // every step's log entry durable. Everything after it is Audit
    // Report generation, whose failure must never affect what already
    // happened here.
    dataset_repo.save(dataset.clone()).await?;
    project_repo.save(project).await?;

    // Part C: automatic, never manually triggered — matches Inspection's
    // import-time auto-trigger exactly. Skipped for a zero-operation
    // call (nothing happened, so nothing to audit — an empty `steps[]`
    // Audit Report would be a report about nothing, not a real one).
    if !audit_steps.is_empty() {
        let _ = generate_and_register_audit_report(
            report_repo,
            dataset.project_id(),
            dataset_id,
            input_dataset_name,
            dataset.name().to_string(),
            recipe_ref,
            audit_steps,
            input_row_count,
            dataset.row_count(),
            input_column_count,
            dataset.schema().columns.len(),
            input_schema_fingerprint,
            operation_log_entry_ids,
            created_at,
            created_by,
        )
        .await;
    }

    Ok(dataset)
}

/// The automatic trigger itself — deliberately NOT `pub`: per the Audit
/// Report task's spec, `generate_audit_report` is never manually
/// triggered by any workspace or command, only called from within
/// `run_transformation_steps` above. Rust's own privacy rules enforce
/// this structurally (no other module can name this function), not just
/// by convention — see `report_audit_tests.rs`'s own structural test for
/// the compiler-independent confirmation of the same fact.
#[allow(clippy::too_many_arguments)]
async fn generate_and_register_audit_report(
    report_repo: &impl ReportRepository,
    project_id: ProjectId,
    dataset_id: DatasetId,
    input_dataset_name: String,
    output_dataset_name: String,
    recipe_ref: Option<(Uuid, u32)>,
    steps: Vec<crate::types::AuditStep>,
    row_count_before: u64,
    row_count_after: u64,
    column_count_before: usize,
    column_count_after: usize,
    input_schema_fingerprint: String,
    operation_log_entry_ids: Vec<Uuid>,
    created_at: Timestamp,
    created_by: String,
) -> Result<RegistrationOutcome, CommandError> {
    let step_count = steps.len();
    let content = crate::types::AuditContent {
        run_summary: format!("{step_count} transformation step(s) applied to Dataset {dataset_id}"),
        input_dataset_id: dataset_id,
        input_dataset_name,
        output_dataset_id: dataset_id,
        output_dataset_name,
        recipe_id: recipe_ref.map(|(id, _)| id),
        recipe_version: recipe_ref.map(|(_, v)| v),
        steps,
        row_count_before,
        row_count_after,
        column_count_before,
        column_count_after,
        transformation_catalogue_version: crate::engines::TRANSFORMATION_CATALOGUE_VERSION.to_string(),
        input_schema_fingerprint,
        operation_log_entry_ids: operation_log_entry_ids.clone(),
    };

    let report_engine = NativeReportEngine;
    // Part D: "transformation_engine" is the real, already-used name of
    // the engine that actually ran this — `NativeTransformationEngine`/
    // `TransformationEngine` (engines/transformation/mod.rs). Checked
    // directly, not guessed: no `recipe_runtime` concept exists anywhere
    // in this codebase — a Recipe is just a named, saved sequence of the
    // same operations (`Recipe`'s own doc comment), applied by the exact
    // same `NativeTransformationEngine`, not a separate runtime — so
    // using a fictional second engine name for the recipe-driven case
    // would misrepresent what actually ran.
    let generated = report_engine.generate_audit_report(content, project_id, Uuid::new_v4(), "transformation_engine".to_string(), created_by.clone(), created_at);

    let content_bytes = serde_json::to_vec(&generated.content).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    let entry_ids_key = operation_log_entry_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let idempotency_key = format!("{dataset_id}:audit_report:{entry_ids_key}:{}", generated.report.content_hash());

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
        log_refs: generated.report.log_refs().to_vec(),
    };

    Ok(report_repo.register_audit_report(request).await?)
}

/// Delegates entirely to `NativeTransformationEngine` via
/// `run_transformation_steps` — this command's only job is Project-
/// context bookkeeping (Part B). Part C: every operation in `operations`
/// appends its OWN Project Operation Log entry — no batching, no
/// exception for a "small" operation (Rule 6/INV-020). No Recipe drove
/// this call (`recipe_ref: None`) — the resulting Audit Report's
/// `recipe_id`/`recipe_version` are `None`, even if `operations` happens
/// to match a Recipe's steps by coincidence.
pub async fn run_transformation(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    operations: Vec<TransformationOperation>,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    run_transformation_steps(dataset_repo, project_repo, report_repo, content_store, dataset_id, operations, None, created_at, created_by).await
}

pub async fn save_pending_plan(
    plan_repo: &impl PendingPlanRepository,
    project_id: ProjectId,
    dataset_id: DatasetId,
    name: String,
    operations: Vec<TransformationOperation>,
    created_at: Timestamp,
) -> Result<PendingTransformationPlan, CommandError> {
    let plan = PendingTransformationPlan::new(Uuid::new_v4(), project_id, dataset_id, name, operations, created_at);
    plan_repo.save(plan.clone()).await?;
    Ok(plan)
}

pub async fn load_pending_plan(plan_repo: &impl PendingPlanRepository, plan_id: Uuid) -> Result<PendingTransformationPlan, CommandError> {
    plan_repo.get(plan_id).await?.ok_or(CommandError::NotFound)
}

/// Applies every operation in the saved plan via `run_transformation` —
/// the exact same per-operation logging path, not a second one, so a
/// plan-run is indistinguishable in the log from the same operations run
/// ad hoc (Part C's "no bypass" concern).
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
    let plan = plan_repo.get(plan_id).await?.ok_or(CommandError::NotFound)?;
    run_transformation(dataset_repo, project_repo, report_repo, content_store, plan.dataset_id(), plan.operations().to_vec(), created_at, created_by).await
}

/// A specific, resolved Recipe version — never "the latest" (matches
/// `ValidationRuleset`/`AnalysisRuleset`'s own already-established
/// discipline: `execution_requires_a_specific_resolved_version_never_an_
/// unresolved_latest`).
pub async fn load_recipe_into_project(knowledge_repo: &impl KnowledgeRepository, recipe_id: Uuid, version: u32) -> Result<RecipeContext, CommandError> {
    let recipe = knowledge_repo.get_recipe(recipe_id).await?.ok_or(CommandError::NotFound)?;
    if recipe.version() != version {
        return Err(ProjectWorkspaceError::RecipeVersionMismatch { requested: version, actual: recipe.version() }.into());
    }
    Ok(RecipeContext { recipe_id: recipe.id(), version: recipe.version(), steps: recipe.steps().to_vec() })
}

/// Delegates to `run_transformation` with the Recipe's `steps` as the
/// operation list — a Recipe is "just a named, saved, replayable
/// sequence" of the same operations (`Recipe`'s own doc comment), so
/// applying one logs exactly like running those operations ad hoc.
pub async fn apply_recipe_to_dataset(
    dataset_repo: &impl DatasetRepository,
    project_repo: &impl ProjectRepository,
    report_repo: &impl ReportRepository,
    content_store: &ParquetContentStore,
    dataset_id: DatasetId,
    recipe_context: RecipeContext,
    created_at: Timestamp,
    created_by: String,
) -> Result<Dataset, CommandError> {
    let recipe_ref = Some((recipe_context.recipe_id, recipe_context.version));
    run_transformation_steps(dataset_repo, project_repo, report_repo, content_store, dataset_id, recipe_context.steps, recipe_ref, created_at, created_by).await
}

/// INV-007: structurally, not just by runtime check, accepts only a
/// `project_id` — there is no parameter through which a Table Event Log
/// id, a Report id, or a Table id could ever reach this function, so
/// "promote from a non-Project-Operation-Log source" is not a call this
/// signature can express, let alone execute.
pub async fn promote_operation_log_to_recipe(
    project_repo: &impl ProjectRepository,
    knowledge_repo: &impl KnowledgeRepository,
    project_id: ProjectId,
    name: String,
    step_range: Range<usize>,
    created_at: Timestamp,
) -> Result<Recipe, CommandError> {
    let project = project_repo.get(project_id).await?.ok_or(CommandError::NotFound)?;
    let entries = project.operation_log().entries();
    let slice = entries.get(step_range.clone()).ok_or(ProjectWorkspaceError::InvalidStepRange)?;

    let steps: Vec<TransformationOperation> = slice.iter().filter_map(|e| e.transformation_operation().cloned()).collect();
    if steps.is_empty() {
        return Err(ProjectWorkspaceError::NoTransformationStepsInRange.into());
    }
    let source_operation_log_entry = slice[0].id();

    let recipe = Recipe::new(Uuid::new_v4(), name, steps, source_operation_log_entry, created_at);
    knowledge_repo.insert_recipe(recipe.clone()).await?;
    Ok(recipe)
}

/// Delegates to the existing Validation Engine with `ValidationTarget::
/// Dataset` and a real `project_id` — largely wiring, since the engine
/// already handles this target type (verified: `dataset_target_requires_
/// project_id_and_routes_to_project_operation_log` already passes).
/// Appends the Project Operation Log's own "run_validation" entry (Part
/// C) — the Validation Engine itself never writes to any log (INV-010).
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
    let mut dataset = dataset_repo.get(dataset_id).await?.ok_or(CommandError::NotFound)?;
    let project_id = dataset.project_id();
    let mut project = project_repo.get(project_id).await?.ok_or(CommandError::NotFound)?;

    let ruleset = knowledge_repo.get_validation_ruleset(ruleset_id).await?.ok_or(CommandError::NotFound)?;
    let version = ruleset.version(ruleset_version_id).ok_or(CommandError::NotFound)?;

    // Execution-time gate (ADR 0008's acknowledged gap, closed here):
    // `activate_version()` only ever checks these at the moment of
    // promotion. Nothing downstream re-checks them, so a version that
    // reached `Active` before this diagnostics engine existed — or one
    // that reaches the system through any future deserialization
    // boundary with a hand-built/corrupted `Active`+`Invalid`
    // combination — must still be caught here rather than trusted on
    // the strength of `version_state` alone.
    if version.version_state() != VersionState::Active {
        return Err(CommandError::ValidationVersionNotActive { version_id: ruleset_version_id, state: version.version_state() });
    }
    if version.definition_status() == DefinitionStatus::Invalid {
        return Err(CommandError::ValidationVersionDefinitionInvalid { version_id: ruleset_version_id });
    }

    let frame = content_store.read(dataset.content_ref()).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    let content = DatasetContent { dataset_id, frame: frame.lazy() };
    let (results, metadata, affected_rows_content) = validate_standard(ValidationTarget::Dataset { content, project_id }, ruleset_id, version, created_by.clone(), created_at, "0.1.0", &|_| false);

    // Part B: every `AffectedRowsRef` the engine just constructed is
    // written for real here — the one place storage I/O happens
    // (INV-008), never inside the engine itself.
    for (reference, mut rows) in affected_rows_content {
        detail_store.write(&reference, &mut rows).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    }

    let report_engine = NativeReportEngine;
    let generated = report_engine.generate_validation_report(results, metadata, version, SourceContext::for_project_dataset(project_id, dataset_id), created_by.clone(), created_at);
    let outcome = register_generated_validation_report(report_repo, &generated).await?;

    if let RegistrationOutcome::Registered { report_id } | RegistrationOutcome::RegisteredButUnavailable { report_id, .. } = outcome {
        dataset.add_report_ref(report_id);
        dataset_repo.save(dataset).await?;
    }
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), project_id, dataset_id, created_at, "run_validation".to_string()));
    project_repo.save(project).await?;

    Ok(outcome)
}

async fn register_generated_validation_report(report_repo: &impl ReportRepository, generated: &crate::engines::report::GeneratedValidationReport) -> Result<RegistrationOutcome, CommandError> {
    let content_bytes = serde_json::to_vec(&generated.content).unwrap_or_default();
    let request = crate::types::report_registration::ValidationReportRegistrationRequest {
        report_registration_request_id: Uuid::new_v4(),
        idempotency_key: format!("{}:validation_report:{}", generated.report.report_id(), generated.report.content_hash()),
        report_id: generated.report.report_id(),
        report_title: generated.report.report_title().to_string(),
        report_schema_version: generated.report.report_schema_version().to_string(),
        report_template_version: generated.report.report_template_version().to_string(),
        created_at: generated.report.created_at(),
        created_by: generated.report.created_by().to_string(),
        source_context: generated.report.source_context().clone(),
        source_event_ref: generated.report.source_event_ref().clone(),
        source_validation_result_id: generated.report.source_validation_result_id(),
        execution_id: generated.report.execution_id(),
        content_format: generated.report.content_format().to_string(),
        content_hash: generated.report.content_hash().to_string(),
        content_bytes,
        knowledge_refs: generated.report.knowledge_refs().to_vec(),
        log_refs: Vec::new(),
    };
    Ok(report_repo.register_validation_report(request).await?)
}

/// Delegates to the existing Analysis Engine with `AnalysisTarget::
/// Dataset` and a real `project_id` — same wiring shape as
/// `run_validation_on_dataset`; see that function's doc comment.
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
    let mut dataset = dataset_repo.get(dataset_id).await?.ok_or(CommandError::NotFound)?;
    let project_id = dataset.project_id();
    let mut project = project_repo.get(project_id).await?.ok_or(CommandError::NotFound)?;

    let ruleset = knowledge_repo.get_analysis_ruleset(ruleset_id).await?.ok_or(CommandError::NotFound)?;
    let version = ruleset.version(ruleset_version_id).ok_or(CommandError::NotFound)?;

    // Execution-time gate — see the identical comment in
    // `run_validation_on_dataset` above; same reasoning, same ADR 0008
    // gap being closed.
    if version.version_state() != AnalysisVersionState::Active {
        return Err(CommandError::AnalysisVersionNotActive { version_id: ruleset_version_id, state: version.version_state() });
    }
    if version.definition_status() == DefinitionStatus::Invalid {
        return Err(CommandError::AnalysisVersionDefinitionInvalid { version_id: ruleset_version_id });
    }

    let frame = content_store.read(dataset.content_ref()).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    let content = DatasetContent { dataset_id, frame: frame.lazy() };
    let (result, metadata, output_table_content) = analyse_standard(AnalysisTarget::Dataset { content, project_id }, ruleset_id, version, created_by.clone(), created_at, "0.1.0", &|_| false);

    // Part B: every `OutputTableRef` the engine just constructed is
    // written for real here — same reasoning as
    // `run_validation_on_dataset` above.
    for (reference, mut table) in output_table_content {
        detail_store.write(&reference, &mut table).map_err(|e| CommandError::Repository(crate::repositories::RepositoryError::Backend(e.to_string())))?;
    }

    let report_engine = NativeReportEngine;
    let generated = report_engine.generate_analysis_report(result, metadata, version, SourceContext::for_project_dataset(project_id, dataset_id), created_by.clone(), created_at);
    let outcome = register_generated_analysis_report(report_repo, &generated).await?;

    if let RegistrationOutcome::Registered { report_id } | RegistrationOutcome::RegisteredButUnavailable { report_id, .. } = outcome {
        dataset.add_report_ref(report_id);
        dataset_repo.save(dataset).await?;
    }
    project.operation_log_mut().append(ProjectOperationLogEntry::new(Uuid::new_v4(), project_id, dataset_id, created_at, "run_analysis".to_string()));
    project_repo.save(project).await?;

    Ok(outcome)
}

async fn register_generated_analysis_report(report_repo: &impl ReportRepository, generated: &crate::engines::report::GeneratedAnalysisReport) -> Result<RegistrationOutcome, CommandError> {
    let content_bytes = serde_json::to_vec(&generated.content).unwrap_or_default();
    let request = crate::types::report_registration::ReportRegistrationRequest {
        report_registration_request_id: Uuid::new_v4(),
        idempotency_key: format!("{}:analysis_report:{}", generated.report.report_id(), generated.report.content_hash()),
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
    Ok(report_repo.register_analysis_report(request).await?)
}
