// INV-022: UI components invoke commands only, never re-implement a rule
// the command layer already enforces. Part C of the workspace UI task
// requires this as a structural check, not just "the tests pass" — so
// this scans the actual component source text rather than only
// exercising behavior.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const COMPONENT_DIR = join(__dirname, "..", "components", "table-workspace");
const PROJECT_COMPONENT_DIR = join(__dirname, "..", "components", "project-workspace");
const SHARED_COMPONENT_DIR = join(__dirname, "..", "components", "shared");
const WORKSPACE_DIR = join(__dirname, "..", "workspaces");

function componentSourceFiles(): { path: string; source: string }[] {
  const dirs = [COMPONENT_DIR, PROJECT_COMPONENT_DIR, SHARED_COMPONENT_DIR, WORKSPACE_DIR];
  const files: { path: string; source: string }[] = [];
  for (const dir of dirs) {
    for (const name of readdirSync(dir)) {
      if (name.endsWith(".tsx") && !name.endsWith(".test.tsx")) {
        files.push({ path: join(dir, name), source: readFileSync(join(dir, name), "utf-8") });
      }
    }
  }
  return files;
}

// Every `disabled={...}` (or `disabled &&`) expression, extracted with
// simple brace-depth matching — good enough for this codebase's JSX
// style, not a full parser.
function disabledExpressions(source: string): string[] {
  const expressions: string[] = [];
  const regex = /disabled=\{/g;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(source))) {
    let depth = 1;
    let i = match.index + match[0].length;
    const start = i;
    while (depth > 0 && i < source.length) {
      if (source[i] === "{") depth++;
      else if (source[i] === "}") depth--;
      i++;
    }
    expressions.push(source.slice(start, i - 1));
  }
  return expressions;
}

const TABLE_STATE_LITERALS = ["Registered", "Available", "Archived", "Unavailable"];

describe("INV-022: no component re-implements the state-action matrix", () => {
  it("every disabled={...} expression is derived from AllowedActions/selection, never a table_state literal comparison", () => {
    const files = componentSourceFiles();
    expect(files.length).toBeGreaterThan(0);

    const offenders: string[] = [];
    for (const { path, source } of files) {
      for (const expr of disabledExpressions(source)) {
        const referencesTableStateField = expr.includes("table_state") || expr.includes("admin_state");
        const referencesStateLiteral = TABLE_STATE_LITERALS.some((literal) => expr.includes(`"${literal}"`));
        if (referencesTableStateField || referencesStateLiteral) {
          offenders.push(`${path}: disabled={${expr}}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("TableNavigator's table_state comparisons are display-only bucketing, never wired to a disabled= attribute", () => {
    const source = readFileSync(join(COMPONENT_DIR, "TableNavigator.tsx"), "utf-8");
    expect(source).toContain("table_state");
    expect(disabledExpressions(source)).toEqual([]);
  });

  it("no component imports or calls table_action_allowed / allowed_actions logic directly — only the AllowedActions query result is consumed", () => {
    const files = componentSourceFiles();
    for (const { path, source } of files) {
      expect(source, `${path} must not reimplement the matrix function itself`).not.toMatch(/table_action_allowed|computeAllowedActions/);
    }
  });

  // Explicitly excluded from the Project Workspace UI task: promote_recipe
  // (its command signature doesn't match the real
  // promote_operation_log_to_recipe implementation) and
  // transform_dataset_many (no command-layer orchestration exists at
  // all). Neither may ever be dispatched for real from any component —
  // structurally, not just by the one component-level test that happens
  // to render the disabled affordance.
  it("promote_recipe is never dispatched for real from any Project Workspace component — invoke('promote_recipe', ...) does not appear anywhere", () => {
    for (const { path, source } of componentSourceFiles()) {
      if (!path.startsWith(PROJECT_COMPONENT_DIR)) continue;
      expect(source, `${path} must not call invoke("promote_recipe", ...)`).not.toMatch(/invoke\(\s*"promote_recipe"/);
    }
  });

  it("transform_dataset_many is never referenced by any Project Workspace component — no multi-dataset transformation UI path exists", () => {
    for (const { path, source } of componentSourceFiles()) {
      if (!path.startsWith(PROJECT_COMPONENT_DIR) && !path.startsWith(WORKSPACE_DIR)) continue;
      expect(source, `${path} must not reference transformDatasetMany / transform_dataset_many`).not.toMatch(/transformDatasetMany|transform_dataset_many/);
    }
  });

  // Report Viewer task, Part B's own requirement: ONE shared component,
  // never a per-workspace reimplementation. This is an import-tracing
  // check, not a behavioral one — it fails if either workspace's Reports
  // tab stops importing the real shared module (e.g. someone copies the
  // file into a workspace-local variant that merely looks the same).
  it("Table Repository's and Project Workspace's Reports tabs both import the one real, shared ReportViewer — never a per-workspace copy", () => {
    const eventLogReportsPanel = readFileSync(join(COMPONENT_DIR, "EventLogReportsPanel.tsx"), "utf-8");
    const projectBottomPanel = readFileSync(join(PROJECT_COMPONENT_DIR, "ProjectBottomPanel.tsx"), "utf-8");
    const importsSharedReportViewer = /import\s*\{\s*ReportViewer\s*\}\s*from\s*"\.\.\/shared\/ReportViewer"/;

    expect(eventLogReportsPanel).toMatch(importsSharedReportViewer);
    expect(projectBottomPanel).toMatch(importsSharedReportViewer);

    // And there is exactly one ReportViewer implementation to import —
    // no second, workspace-local "ReportViewer"-named component exists
    // anywhere else in the tree that either file could have imported
    // instead.
    const allFiles = componentSourceFiles();
    const reportViewerDefinitions = allFiles.filter((f) => /export function ReportViewer\(/.test(f.source));
    expect(reportViewerDefinitions.map((f) => f.path)).toEqual([join(SHARED_COMPONENT_DIR, "ReportViewer.tsx")]);
  });
});
