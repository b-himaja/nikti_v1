// INV-022: UI components call these typed wrappers, never Tauri's raw
// `invoke` and never an engine/repository directly. INV-023: each wrapper
// here is the one canonical entry point for its action — every menu item,
// button, or shortcut that performs it calls the same function.
//
// No real request/response shapes are implemented yet — these mirror the
// nikti-core command signatures (crates/nikti-core/src/commands/commands.rs)
// so the frontend and backend command sets stay in lockstep, but the actual
// domain payload types land alongside the real command implementations.

import { invoke } from "@tauri-apps/api/core";

export type ComputeBackend =
  | { Native: null }
  | { Python: { status: PythonSidecarStatus } };

export type PythonSidecarStatus =
  | { Ready: { python_env_hash: string } }
  | { Unavailable: { reason: string } };

// Mirrors nikti_core::types::TransformationOperation
// (crates/nikti-core/src/types/transformation_operation.rs) field-for-field,
// in serde's default externally-tagged shape — kept in lockstep per
// INV-023, same as the rest of this file.
export type ScalarValue =
  | { Null: null }
  | { Bool: boolean }
  | { Int: number }
  | { Float: number }
  | { Str: string };

export type TransformationOperation =
  | { DeriveColumn: { output_col: string; formula: string; overwrite: boolean } }
  | {
      RecodeColumn: {
        column: string;
        mapping: [string, string][];
        default: string | null;
        output_col: string | null;
      };
    }
  | { RenameColumns: { mapping: [string, string][] } }
  | { ReorderColumns: { column_order: string[] } }
  | { DropColumns: { columns: string[] } }
  | { CopyColumn: { column: string; output_col: string } }
  | { CoalesceColumns: { columns: string[]; output_col: string } }
  | { GenerateRowNumber: { output_col: string | null; start: number } }
  | { GenerateUuid: { output_col: string | null; seed: number | null } }
  | { NullStandardise: { columns: string[]; null_variants: string[] | null } }
  | {
      TextStandardise: {
        column: string;
        output_col: string | null;
        mapping: [string, string][];
        case_sensitive: boolean;
      };
    }
  | { HashColumn: { column: string; output_col: string | null; algorithm: "Sha256" | "Md5" | "Sha1" } }
  | { Differencing: { column: string; output_col: string | null; periods: number; pct_change: boolean } }
  // Remaining variants (Discretise/LeadLag/FillMissing/ApplyScaler/StringOp/
  // DatetimeOp/ConditionalFlag/FlagValues/ChangeDtype/Join/Append/Intersect/
  // Subtract/Compare/Unpivot/Aggregate/MovingCalc/RunningTotal/Filter/
  // Sample/Sort/Slice/Delete/Tag) carry nested parameter shapes not yet
  // needed by the placeholder UI — add as each gets a real workspace
  // control, matching the Rust enum exactly rather than guessing its shape.
  | { [variant: string]: unknown };

// `nikti_core::types::Recipe` (crates/nikti-core/src/types/recipe.rs) —
// used only for `promoteRecipe`'s signature lockstep (see below); no
// component constructs or consumes one for real yet.
export interface Recipe {
  id: string;
  name: string;
  version: number;
  steps: TransformationOperation[];
  source_operation_log_entry: string;
  updated_at: number;
}

export function importTable(uri: string): Promise<unknown> {
  return invoke("import_table", { uri });
}

export function openTable(tableId: string): Promise<unknown> {
  return invoke("open_table", { tableId });
}

export function inspectTable(tableId: string): Promise<unknown> {
  return invoke("inspect_table", { tableId });
}

export function validateTable(tableId: string, rulesetId: string): Promise<unknown> {
  return invoke("validate_table", { tableId, rulesetId });
}

export function analyseNative(tableId: string, rulesetId: string): Promise<unknown> {
  return invoke("analyse_native", { tableId, rulesetId });
}

export function analysePython(
  tableId: string,
  rulesetId: string,
  backend: ComputeBackend,
): Promise<unknown> {
  return invoke("analyse_python", { tableId, rulesetId, backend });
}

// `transform_dataset`/`apply_recipe` return types corrected to `Dataset`
// — `commands::transform_dataset`/`commands::apply_recipe` both really
// return `Result<Dataset, CommandError>` (project_workspace.rs), this
// was `unknown` only because this file predates the Project Workspace
// backend task; `Dataset` is now defined below, so this can be accurate.
export function transformDataset(datasetId: string, operation: TransformationOperation): Promise<Dataset> {
  return invoke("transform_dataset", { datasetId, operation });
}

// GENUINE GAP, not wired by this task — `commands::transform_dataset_many`
// is still `todo!()` (no command-layer orchestration exists for it yet,
// only the engine-level `transform_many` is real). This wrapper exists
// only so its type is in lockstep with the Rust signature per INV-023;
// calling it against the real backend today would panic. No UI in this
// app calls it.
export function transformDatasetMany(
  datasetIds: string[],
  operation: TransformationOperation,
): Promise<Dataset> {
  return invoke("transform_dataset_many", { datasetIds, operation });
}

export function applyRecipe(datasetId: string, recipeId: string): Promise<Dataset> {
  return invoke("apply_recipe", { datasetId, recipeId });
}

// GENUINE GAP, not wired by this task — `commands::promote_recipe` is
// still `todo!()` (its signature doesn't match the real
// `promote_operation_log_to_recipe`, which needs a project-owning-this-
// log-entry reverse lookup that doesn't exist). This wrapper exists only
// for signature lockstep per INV-023; calling it against the real
// backend today would panic. No UI in this app calls it — where a
// "Promote to Recipe" affordance is shown, it is always disabled.
export function promoteRecipe(sourceOperationLogEntry: string): Promise<Recipe> {
  return invoke("promote_recipe", { sourceOperationLogEntry });
}

// GENUINE GAP, not wired by this task — no UI component calls this yet
// (the Lineage Report task built the real backend only). Kept in
// signature lockstep with `commands::generate_lineage_report` /
// `ipc_commands::generate_lineage_report` per INV-023, same discipline
// as `promoteRecipe`/`transformDatasetMany` above. Exactly one of
// `tableId`/`datasetId` must be provided — mirrors
// `commands::LineageQueryObject`'s two real variants.
export function generateLineageReport(tableId: string | null, datasetId: string | null): Promise<RegistrationOutcome> {
  return invoke("generate_lineage_report", { tableId, datasetId });
}

export function generateComparison(a: string, b: string): Promise<unknown> {
  return invoke("generate_comparison", { a, b });
}

export function purgeTable(tableId: string, reason: string): Promise<unknown> {
  return invoke("purge_table", { tableId, reason });
}

export function createProject(name: string): Promise<unknown> {
  return invoke("create_project", { name });
}

export function createDataset(
  projectId: string,
  sourceTableId: string | null,
  name: string,
): Promise<unknown> {
  return invoke("create_dataset", { projectId, sourceTableId, name });
}

// --- Table Repository Workspace (v2) ---
// Mirrors crates/nikti-core/src/types/table.rs field-for-field (the
// canonical nested Table Object Model, Part C) and
// crates/nikti-core/src/commands/{commands.rs,table_workspace.rs} for the
// command surface. Field names stay snake_case to match serde's default
// (no `rename_all` on these Rust types) — only invoke() argument KEYS are
// camelCase, per Tauri's own convention, matching every function above.

export type TableState = "Registered" | "Available" | "Archived" | "Unavailable";
export type CreationSourceType = "Import" | "ProjectDatasetExport";
export type TableSourceType =
  | "Csv"
  | "Excel"
  | "DatabaseTable"
  | "DatabaseQuery"
  | "Parquet"
  | "Json"
  | "ProjectDataset"
  | "Other";

export interface ColumnDef {
  name: string;
  data_type: string;
}

export interface TableSchema {
  columns: ColumnDef[];
}

export interface CreationSource {
  creation_source_type: CreationSourceType;
  source_type: TableSourceType;
}

export interface SourceProvenance {
  source_label: string;
  source_project_id: string | null;
  source_dataset_id: string | null;
}

export interface TableDataRef {
  content_ref: string;
  data_fingerprint: string;
  row_count: number;
}

export interface SchemaRef {
  schema: TableSchema;
  schema_fingerprint: string;
}

export interface VersionInfo {
  version_family_id: string;
}

export interface DescriptiveMetadata {
  table_name: string;
  data_dictionary_id: string | null;
}

export interface AdminState {
  table_state: TableState;
  table_event_log_id: string | null;
  archived_at: number | null;
}

// The canonical nested Table Object (Part C) — every field here mirrors
// `Table`'s own private fields via serde, not a flattened re-derivation.
export interface Table {
  id: string;
  creation_source: CreationSource;
  source_provenance: SourceProvenance;
  table_data_ref: TableDataRef;
  schema_ref: SchemaRef;
  version_info: VersionInfo;
  descriptive_metadata: DescriptiveMetadata;
  admin_state: AdminState;
  created_at: number;
  created_by: string;
}

export interface TableEventLogEntry {
  id: string;
  table_id: string;
  recorded_at: number;
  event: string;
}

export interface TableEventLog {
  entries: TableEventLogEntry[];
}

export interface SourceContext {
  source_context_type: "Table" | "ProjectDataset" | "Project";
  table_id: string | null;
  project_dataset_id: string | null;
  project_id: string | null;
}

export interface EngineVersion {
  rust_core: string;
  python_env_hash: string | null;
}

export interface SourceEventRef {
  source_event_type: string;
  source_execution_id: string;
  source_engine: string;
  source_engine_version: EngineVersion;
  source_execution_status: string;
}

// The single source of truth for this frontend's mirror of
// `nikti_core::types::common::ReportKind` — every place that needs "all
// report kinds" (the `report_type` field below, `AnyReport`'s variant
// keys) derives from this one array instead of retyping the list a
// second time. `src/test/reportKind.exhaustiveness.test.ts` parses the
// real Rust enum directly and asserts it matches this array exactly, so
// a variant added on the Rust side and missed here fails that test
// instead of silently compiling — this is what status audit 3 found
// missing (`"Comparison"` was absent from both places below, undetected
// because nothing tied them back to the real enum).
export const REPORT_KINDS = ["Inspection", "Validation", "Analysis", "Audit", "Lineage", "Comparison"] as const;
export type ReportKindName = (typeof REPORT_KINDS)[number];

// Shared Report Object Model v2 baseline (Inspection/Validation/Analysis
// all carry these fields — see report_inspection.rs/report_validation.rs/
// report_analysis.rs). Validation additionally carries
// report_template_version/source_validation_result_id/execution_id.
export interface ReportBaseline {
  report_id: string;
  report_type: ReportKindName;
  report_title: string;
  report_state: "Registered" | "Available" | "Unavailable";
  report_schema_version: string;
  created_at: number;
  created_by: string;
  generated_by_engine: string;
  source_context: SourceContext;
  source_event_ref: SourceEventRef;
  content_ref: string;
  content_format: string;
  content_hash: string;
  repository_registered_at: number | null;
  knowledge_refs: string[];
  log_refs: string[];
}

// `nikti_core::repositories::AnyReport` — an externally-tagged Rust enum;
// serde wraps each variant's payload under its variant name. The variant
// keys below (Inspection/Validation/Analysis/Audit/Lineage/Comparison)
// must cover exactly `ReportKindName` — checked at compile time by
// `AssertAnyReportKeysCoverReportKinds` and at the Rust-source level by
// the exhaustiveness test referenced above.
export type AnyReport =
  | { Inspection: ReportBaseline }
  | { Validation: ReportBaseline & { report_template_version: string; source_validation_result_id: string; execution_id: string } }
  | { Analysis: ReportBaseline }
  | { Audit: Record<string, unknown> }
  | { Lineage: Record<string, unknown> }
  | { Comparison: ReportBaseline & { compared_report_a_id: string; compared_report_b_id: string } };

// Exported for `ReportViewer` (Audit #3 cleanup task, Part B): switching
// on this type instead of a bare `string` gives the render-path its own
// compiler-enforced exhaustiveness check, the same mechanism this file
// already uses for `AnyReport`'s own variant keys just above.
export type AnyReportKind = AnyReport extends infer R ? (R extends object ? keyof R : never) : never;
type AssertExactly<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
// If `AnyReport`'s variant keys and `ReportKindName` ever diverge in
// either direction, this line fails to compile (`tsc --noEmit`) rather
// than silently drifting — the same guarantee Rust's own exhaustive
// `match` gives the backend's `content_ref_of`.
const _assertAnyReportKeysCoverReportKinds: AssertExactly<AnyReportKind, ReportKindName> = true;
void _assertAnyReportKeysCoverReportKinds;

export interface DatasetSnapshot {
  index: number;
  content_ref: string;
  content_fingerprint: string;
  schema: TableSchema;
  row_count: number;
  operation: string;
  note: string | null;
  created_at: number;
}

// Mirrors nikti_core::types::Dataset (crates/nikti-core/src/types/dataset.rs)
// field-for-field — corrected here to include `source_dataset_id`,
// `content_ref`, and `report_refs`, which the Project Workspace backend
// task added after this interface was first written for the Table
// Repository UI task and which nothing since had kept in sync.
export interface Dataset {
  id: string;
  project_id: string;
  name: string;
  source_table_id: string | null;
  source_dataset_id: string | null;
  schema: TableSchema;
  row_count: number;
  content_ref: string;
  content_fingerprint: string;
  report_refs: string[];
  snapshots: DatasetSnapshot[];
  snapshot_counter: number;
  updated_at: number;
}

// Mirrors nikti_core::types::ProjectOperationLogEntry/ProjectOperationLog
// (crates/nikti-core/src/types/project_operation_log.rs). INV-004/005:
// append-only — no field here is ever mutated once appended, and no UI
// affordance may imply otherwise (INV-021).
export interface ProjectOperationLogEntry {
  id: string;
  project_id: string;
  dataset_id: string;
  recorded_at: number;
  operation: string;
  transformation_operation: TransformationOperation | null;
}

export interface ProjectOperationLog {
  entries: ProjectOperationLogEntry[];
}

export interface Project {
  id: string;
  name: string;
  created_at: number;
  dataset_ids: string[];
  operation_log: ProjectOperationLog;
}

// `nikti_core::commands::project_workspace::ProjectSummary` — a flattened
// summary for `list_projects`, never a second competing object model.
export interface ProjectSummary {
  project_id: string;
  name: string;
  dataset_count: number;
  created_at: number;
}

// `nikti_core::commands::project_workspace::RecipeContext` — a specific,
// resolved Recipe version ready to apply, never "the latest".
export interface RecipeContext {
  recipe_id: string;
  version: number;
  steps: TransformationOperation[];
}

// `nikti_core::types::PendingTransformationPlan`
// (crates/nikti-core/src/types/pending_transformation_plan.rs) — a bare
// Working Object with NO `plan_state`/`definition_status` field. Do not
// add one here that the real Rust struct doesn't have.
export interface PendingTransformationPlan {
  id: string;
  project_id: string;
  dataset_id: string;
  name: string;
  operations: TransformationOperation[];
  created_at: number;
}

// `nikti_core::types::RegistrationOutcome`
// (crates/nikti-core/src/types/report_registration.rs) — externally
// tagged, matching serde's default enum representation.
export type RegistrationOutcome =
  | { Registered: { report_id: string } }
  | { RegisteredButUnavailable: { report_id: string; reason: string } }
  | { Idempotent: { report_id: string } };

// `nikti_core::commands::table_workspace::DictionaryNavigation` — mixed
// unit/struct-variant enum: serde serializes the unit variant as a bare
// string and the struct variant wrapped under its name.
export type DictionaryNavigation = "NoDictionaryReferenced" | { ServiceNotYetAvailable: { data_dictionary_id: string } };

export type RulesetManagerTarget = "Validation" | "Analysis";

export interface RulesetManagerNavigation {
  table_id: string;
  target: RulesetManagerTarget;
  ruleset_version_id: string | null;
  requires_ruleset_selection: boolean;
}

export function importTableWorkspace(uri: string): Promise<Table> {
  return invoke("import_table", { uri });
}

export function reimportTableAsNew(originalTableId: string, uri: string): Promise<Table> {
  return invoke("reimport_table_as_new", { originalTableId, uri });
}

export function searchTables(query: string): Promise<Table[]> {
  return invoke("search_tables", { query });
}

export function clearTableSearch(): Promise<Table[]> {
  return invoke("clear_table_search", {});
}

export function refreshTableRepository(): Promise<Table[]> {
  return invoke("refresh_table_repository", {});
}

export function openTableReadonly(tableId: string): Promise<Table> {
  return invoke("open_table_readonly", { tableId });
}

export function openTableInformation(tableId: string): Promise<Table> {
  return invoke("open_table_information", { tableId });
}

export function openTableDictionary(tableId: string): Promise<DictionaryNavigation> {
  return invoke("open_table_dictionary", { tableId });
}

export function openTableEventLog(tableId: string): Promise<TableEventLog> {
  return invoke("open_table_event_log", { tableId });
}

export function openTableReports(tableId: string): Promise<AnyReport[]> {
  return invoke("open_table_reports", { tableId });
}

export function loadTableIntoProject(tableId: string, projectId: string): Promise<Dataset> {
  return invoke("load_table_into_project", { tableId, projectId });
}

export function createProjectFromTable(tableId: string, newProjectName: string): Promise<[Project, Dataset]> {
  return invoke("create_project_from_table", { tableId, newProjectName });
}

export function routeTableInspection(tableId: string): Promise<unknown> {
  return invoke("route_table_inspection", { tableId });
}

export function routeTableValidation(tableId: string, rulesetVersionId: string | null): Promise<RulesetManagerNavigation> {
  return invoke("route_table_validation", { tableId, rulesetVersionId });
}

export function routeTableAnalysis(tableId: string, rulesetVersionId: string | null): Promise<RulesetManagerNavigation> {
  return invoke("route_table_analysis", { tableId, rulesetVersionId });
}

export function archiveTable(tableId: string): Promise<Table> {
  return invoke("archive_table", { tableId });
}

export function restoreTable(tableId: string): Promise<Table> {
  return invoke("restore_table", { tableId });
}

export function retryTableAvailabilityCheck(tableId: string): Promise<Table> {
  return invoke("retry_table_availability_check", { tableId });
}

export function copyTableId(tableId: string): Promise<string> {
  return invoke("copy_table_id", { tableId });
}

// Not one of the 19 action commands — a read-only query so the UI can
// reflect Part H's state-action matrix (disabled buttons/tooltips)
// without re-deriving the rule itself. See
// `commands::table_workspace::allowed_actions` (Rust) for the single
// source of truth this returns verbatim.
export interface AllowedActions {
  open_readonly: boolean;
  load_into_project: boolean;
  route_inspection: boolean;
  route_validation: boolean;
  route_analysis: boolean;
  archive: boolean;
  restore: boolean;
}

export function tableAllowedActions(tableId: string): Promise<AllowedActions> {
  return invoke("table_allowed_actions", { tableId });
}

// --- Project Workspace (v2) ---
// Mirrors crates/nikti-core/src/commands/project_workspace.rs's public
// API, wired through the canonical `commands::*` delegations added
// alongside these wrappers — same one-command-per-action discipline as
// the Table Repository section above.

// `nikti_core::commands::ReportWithContent` — the Report Viewer's one
// canonical response shape, real metadata (the same `AnyReport` shape
// `openTableReports` already returns per entry) plus the real parsed
// content body. `content` is genuinely `unknown` here, not a guessed
// shape: each report kind's body differs (rule_results for Validation,
// procedure_results for Analysis, etc.) and the command layer
// deliberately returns generic parsed JSON rather than a per-kind typed
// model (see `open_report`'s own doc comment in commands.rs).
export interface ReportWithContent {
  report: AnyReport;
  content: unknown;
}

export function openReport(reportId: string): Promise<ReportWithContent> {
  return invoke("open_report", { reportId });
}

export function openProject(projectId: string): Promise<Project> {
  return invoke("open_project", { projectId });
}

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects", {});
}

export function duplicateDataset(datasetId: string, name: string): Promise<Dataset> {
  return invoke("duplicate_dataset", { datasetId, name });
}

export function exportDatasetAsTable(datasetId: string, tableName: string): Promise<Table> {
  return invoke("export_dataset_as_table", { datasetId, tableName });
}

export function savePendingPlan(projectId: string, datasetId: string, name: string, operations: TransformationOperation[]): Promise<PendingTransformationPlan> {
  return invoke("save_pending_plan", { projectId, datasetId, name, operations });
}

export function loadPendingPlan(planId: string): Promise<PendingTransformationPlan> {
  return invoke("load_pending_plan", { planId });
}

export function runPendingPlan(planId: string): Promise<Dataset> {
  return invoke("run_pending_plan", { planId });
}

export function loadRecipeIntoProject(recipeId: string, version: number): Promise<RecipeContext> {
  return invoke("load_recipe_into_project", { recipeId, version });
}

export function runValidationOnDataset(datasetId: string, rulesetId: string, rulesetVersionId: string): Promise<RegistrationOutcome> {
  return invoke("run_validation_on_dataset", { datasetId, rulesetId, rulesetVersionId });
}

export function runAnalysisOnDataset(datasetId: string, rulesetId: string, rulesetVersionId: string): Promise<RegistrationOutcome> {
  return invoke("run_analysis_on_dataset", { datasetId, rulesetId, rulesetVersionId });
}
