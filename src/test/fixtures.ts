// Fixtures shaped to match the REAL Rust types byte-for-byte (field
// names, nesting, and serde's default enum representation) — not an
// invented "looks plausible" shape. Cross-referenced against:
//   - crates/nikti-core/src/types/table.rs (Table + nested objects)
//   - crates/nikti-core/src/types/table_event_log.rs
//   - crates/nikti-core/src/types/report_inspection.rs (Report Object
//     Model v2 baseline, shared by Inspection/Validation/Analysis)
//   - crates/nikti-core/src/commands/table_workspace.rs (AllowedActions,
//     DictionaryNavigation, RulesetManagerNavigation)
// This is the closest honest substitute for "the command's real response
// shape" available without a live Tauri/webview process — a jsdom unit
// test has no OS webview or Rust child process to invoke for real.

import type {
  AllowedActions,
  AnyReport,
  Dataset,
  PendingTransformationPlan,
  Project,
  ProjectOperationLogEntry,
  ProjectSummary,
  RecipeContext,
  RegistrationOutcome,
  ReportWithContent,
  Table,
  TableEventLog,
} from "../utils/ipc";

export function makeTable(overrides: Partial<Table> = {}): Table {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    creation_source: { creation_source_type: "Import", source_type: "Csv" },
    source_provenance: { source_label: "orders.csv", source_project_id: null, source_dataset_id: null },
    table_data_ref: { content_ref: "table-content/fp-data.parquet", data_fingerprint: "fp-data", row_count: 3 },
    schema_ref: { schema: { columns: [{ name: "id", data_type: "Int64" }] }, schema_fingerprint: "fp-schema" },
    version_info: { version_family_id: "22222222-2222-4222-8222-222222222222" },
    descriptive_metadata: { table_name: "orders", data_dictionary_id: null },
    admin_state: { table_state: "Available", table_event_log_id: "33333333-3333-4333-8333-333333333333", archived_at: null },
    created_at: 1_700_000_000_000,
    created_by: "tester",
    ...overrides,
  };
}

export function makeEventLog(): TableEventLog {
  return {
    entries: [{ id: "33333333-3333-4333-8333-333333333333", table_id: "11111111-1111-4111-8111-111111111111", recorded_at: 1_700_000_000_000, event: "imported" }],
  };
}

export function makeInspectionReport(): AnyReport {
  return {
    Inspection: {
      report_id: "44444444-4444-4444-8444-444444444444",
      report_type: "Inspection",
      report_title: "Inspection Report",
      report_state: "Available",
      report_schema_version: "1.0",
      created_at: 1_700_000_000_000,
      created_by: "tester",
      generated_by_engine: "report_engine",
      source_context: { source_context_type: "Table", table_id: "11111111-1111-4111-8111-111111111111", project_dataset_id: null, project_id: null },
      source_event_ref: {
        source_event_type: "inspection_run",
        source_execution_id: "55555555-5555-4555-8555-555555555555",
        source_engine: "inspection_engine",
        source_engine_version: { rust_core: "0.1.0", python_env_hash: null },
        source_execution_status: "completed",
      },
      content_ref: "inspection-report-content/abc.json",
      content_format: "application/json",
      content_hash: "abc",
      repository_registered_at: 1_700_000_000_000,
      knowledge_refs: [],
      log_refs: ["33333333-3333-4333-8333-333333333333"],
    },
  };
}

// Same baseline shape as `makeInspectionReport`, plus Comparison's own
// two extra fields (`compared_report_a_id`/`compared_report_b_id`) —
// cross-referenced against report_comparison.rs's `ComparisonReport`.
export function makeComparisonReport(): AnyReport {
  return {
    Comparison: {
      report_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
      report_type: "Comparison",
      report_title: "Comparison Report",
      report_state: "Available",
      report_schema_version: "1.0",
      created_at: 1_700_000_000_000,
      created_by: "tester",
      generated_by_engine: "report_engine",
      source_context: { source_context_type: "Table", table_id: null, project_dataset_id: null, project_id: null },
      source_event_ref: {
        source_event_type: "engine_execution",
        source_execution_id: "55555555-5555-4555-8555-555555555555",
        source_engine: "report_engine",
        source_engine_version: { rust_core: "0.1.0", python_env_hash: null },
        source_execution_status: "completed",
      },
      content_ref: "comparison-report-content/def.json",
      content_format: "application/json",
      content_hash: "def",
      repository_registered_at: 1_700_000_000_000,
      knowledge_refs: [],
      log_refs: [],
      compared_report_a_id: "44444444-4444-4444-8444-444444444444",
      compared_report_b_id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
    },
  };
}

// Cross-referenced against report_lineage.rs's `LineageReport` (no
// extra fields beyond the shared baseline).
export function makeLineageReport(): AnyReport {
  return {
    Lineage: {
      report_id: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
      report_type: "Lineage",
      report_title: "Lineage Report",
      report_state: "Available",
      report_schema_version: "1.0",
      created_at: 1_700_000_000_000,
      created_by: "tester",
      generated_by_engine: "report_engine",
      source_context: { source_context_type: "Table", table_id: "11111111-1111-4111-8111-111111111111", project_dataset_id: null, project_id: null },
      source_event_ref: {
        source_event_type: "engine_execution",
        source_execution_id: "55555555-5555-4555-8555-555555555555",
        source_engine: "report_engine",
        source_engine_version: { rust_core: "0.1.0", python_env_hash: null },
        source_execution_status: "completed",
      },
      content_ref: "lineage-report-content/ghi.json",
      content_format: "application/json",
      content_hash: "ghi",
      repository_registered_at: 1_700_000_000_000,
      knowledge_refs: [],
      log_refs: [],
    },
  };
}

export function makeAllowedActions(overrides: Partial<AllowedActions> = {}): AllowedActions {
  return {
    open_readonly: true,
    load_into_project: true,
    route_inspection: true,
    route_validation: true,
    route_analysis: true,
    archive: true,
    restore: false,
    ...overrides,
  };
}

// The exact truth table from `table_action_allowed`
// (crates/nikti-core/src/commands/table_workspace.rs) with
// `archived_open_policy = true` — mirrored here, not re-derived, so the
// UI-side matrix test in Toolbar.test.tsx can assert against the same
// ground truth the Rust test `part_h_state_action_matrix_is_fully_enforced`
// checks.
export const ALLOWED_ACTIONS_BY_STATE: Record<Table["admin_state"]["table_state"], AllowedActions> = {
  Registered: { open_readonly: false, load_into_project: false, route_inspection: false, route_validation: false, route_analysis: false, archive: false, restore: false },
  Available: { open_readonly: true, load_into_project: true, route_inspection: true, route_validation: true, route_analysis: true, archive: true, restore: false },
  Archived: { open_readonly: true, load_into_project: false, route_inspection: false, route_validation: false, route_analysis: false, archive: false, restore: true },
  Unavailable: { open_readonly: false, load_into_project: false, route_inspection: false, route_validation: false, route_analysis: false, archive: false, restore: false },
};

// Project Workspace fixtures — cross-referenced against
// crates/nikti-core/src/types/dataset.rs, project.rs,
// project_operation_log.rs and
// crates/nikti-core/src/commands/project_workspace.rs (ProjectSummary,
// RecipeContext), same "real shape, not invented" discipline as above.

export function makeDataset(overrides: Partial<Dataset> = {}): Dataset {
  return {
    id: "66666666-6666-4666-8666-666666666666",
    project_id: "77777777-7777-4777-8777-777777777777",
    name: "orders",
    source_table_id: "11111111-1111-4111-8111-111111111111",
    source_dataset_id: null,
    schema: { columns: [{ name: "id", data_type: "Int64" }] },
    row_count: 3,
    content_ref: "dataset-content/fp-data.parquet",
    content_fingerprint: "fp-data",
    report_refs: [],
    snapshots: [],
    snapshot_counter: 0,
    updated_at: 1_700_000_000_000,
    ...overrides,
  };
}

export function makeProjectOperationLogEntry(overrides: Partial<ProjectOperationLogEntry> = {}): ProjectOperationLogEntry {
  return {
    id: "88888888-8888-4888-8888-888888888888",
    project_id: "77777777-7777-4777-8777-777777777777",
    dataset_id: "66666666-6666-4666-8666-666666666666",
    recorded_at: 1_700_000_000_000,
    operation: "load",
    transformation_operation: null,
    ...overrides,
  };
}

export function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    id: "77777777-7777-4777-8777-777777777777",
    name: "Orders Project",
    created_at: 1_700_000_000_000,
    dataset_ids: ["66666666-6666-4666-8666-666666666666"],
    operation_log: { entries: [makeProjectOperationLogEntry()] },
    ...overrides,
  };
}

export function makeProjectSummary(overrides: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    project_id: "77777777-7777-4777-8777-777777777777",
    name: "Orders Project",
    dataset_count: 1,
    created_at: 1_700_000_000_000,
    ...overrides,
  };
}

export function makeRecipeContext(overrides: Partial<RecipeContext> = {}): RecipeContext {
  return {
    recipe_id: "99999999-9999-4999-8999-999999999999",
    version: 1,
    steps: [{ DropColumns: { columns: ["amount"] } }],
    ...overrides,
  };
}

export function makePendingPlan(overrides: Partial<PendingTransformationPlan> = {}): PendingTransformationPlan {
  return {
    id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    project_id: "77777777-7777-4777-8777-777777777777",
    dataset_id: "66666666-6666-4666-8666-666666666666",
    name: "pending plan",
    operations: [{ DropColumns: { columns: ["amount"] } }],
    created_at: 1_700_000_000_000,
    ...overrides,
  };
}

export function makeRegistrationOutcome(overrides: Partial<{ report_id: string }> = {}): RegistrationOutcome {
  return { Registered: { report_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", ...overrides } };
}

// `nikti_core::commands::ReportWithContent` — real metadata (the same
// AnyReport shape openTableReports returns) plus real parsed content.
export function makeReportWithContent(overrides: Partial<ReportWithContent> = {}): ReportWithContent {
  return {
    report: makeInspectionReport(),
    content: { measurements: { row_count: 3, column_count: 2 } },
    ...overrides,
  };
}
