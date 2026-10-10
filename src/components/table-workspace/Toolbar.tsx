import { useEffect, useState } from "react";

import {
  archiveTable,
  copyTableId,
  createProjectFromTable,
  listProjects,
  loadTableIntoProject,
  openTableDictionary,
  openTableReadonly,
  reimportTableAsNew,
  restoreTable,
  retryTableAvailabilityCheck,
  routeTableAnalysis,
  routeTableInspection,
  routeTableValidation,
} from "../../utils/ipc";
import { importTableWorkspace } from "../../utils/ipc";
import type { AllowedActions, Dataset, ProjectSummary } from "../../utils/ipc";
import { useUiStore } from "../../store/uiStore";

interface Props {
  tableId: string | null;
  allowed: AllowedActions | null;
  onAfterAction: () => void;
}

// Part B/Part C: every button here dispatches exactly one canonical
// command — none contains inline logic. Enable/disable state is read
// straight off `AllowedActions` (fetched from the backend, see
// TableRepositoryWorkspace) — this component makes no allow/deny
// decision of its own; it only reflects what the command layer already
// decided. `table.search`/`table.clear_search`/`table.refresh_repository`
// live in TableNavigator and `table.open_event_log`/`table.open_reports`
// live in EventLogReportsPanel (their tabs ARE the dispatch) — putting a
// second button here for the same three would be a second UI entry point
// for the same command, not new logic, but redundant, so they're omitted
// here per Part B's own component split.
export function Toolbar({ tableId, allowed, onAfterAction }: Props) {
  const [message, setMessage] = useState("(no command run yet)");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string>("");
  const navigateToProject = useUiStore((s) => s.navigateToProject);

  // Real target-project list for table.load_into_project — previously
  // this action had no way to supply a real project id at all (it called
  // the command with a hardcoded empty string, which the backend
  // correctly rejects as not-found). table.create_project_from_table
  // needs no such list — it always creates a brand-new Project.
  useEffect(() => {
    listProjects().then(setProjects);
  }, []);

  async function run(label: string, fn: () => Promise<unknown>) {
    try {
      const result = await fn();
      setMessage(`${label}: ${JSON.stringify(result)}`);
      return result;
    } catch (err) {
      // Part D: honest incompleteness — e.g. table.import/reimport_as_new
      // surface as errors until a CSV-parsing pipeline exists; this is
      // never hidden or silently retried.
      setMessage(`${label}: ${String(err)}`);
      return null;
    } finally {
      onAfterAction();
    }
  }

  const noSelection = !tableId;

  return (
    <div role="toolbar" aria-label="Table Repository Workspace toolbar">
      <button type="button" onClick={() => run("table.import", () => importTableWorkspace(""))}>
        table.import
      </button>
      <button type="button" disabled={noSelection} onClick={() => tableId && run("table.reimport_as_new", () => reimportTableAsNew(tableId, ""))}>
        table.reimport_as_new
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.open_readonly}
        title={allowed && !allowed.open_readonly ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.open_readonly", () => openTableReadonly(tableId))}
      >
        table.open_readonly
      </button>
      <button type="button" disabled={noSelection} onClick={() => tableId && run("table.open_dictionary", () => openTableDictionary(tableId))}>
        table.open_dictionary
      </button>
      <select aria-label="Target project for table.load_into_project" value={selectedProjectId} onChange={(e) => setSelectedProjectId(e.target.value)}>
        <option value="">(select a project)</option>
        {projects.map((p) => (
          <option key={p.project_id} value={p.project_id}>
            {p.name}
          </option>
        ))}
      </select>
      <button
        type="button"
        disabled={noSelection || !allowed?.load_into_project || !selectedProjectId}
        title={allowed && !allowed.load_into_project ? "Not allowed for this Table's current state" : undefined}
        onClick={async () => {
          if (!tableId || !selectedProjectId) return;
          const dataset = await run("table.load_into_project", () => loadTableIntoProject(tableId, selectedProjectId));
          // Part B: real navigation to the newly-created Dataset, not
          // just a JSON dump of the result — the actual hand-off.
          if (dataset && typeof dataset === "object" && "id" in dataset) {
            navigateToProject(selectedProjectId, dataset as Dataset);
          }
        }}
      >
        table.load_into_project
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.load_into_project}
        title={allowed && !allowed.load_into_project ? "Not allowed for this Table's current state" : undefined}
        onClick={async () => {
          if (!tableId) return;
          const outcome = await run("table.create_project_from_table", () => createProjectFromTable(tableId, "New Project"));
          if (Array.isArray(outcome) && outcome.length === 2) {
            const [project, dataset] = outcome as [{ id: string }, Dataset];
            navigateToProject(project.id, dataset);
          }
        }}
      >
        table.create_project_from_table
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.route_inspection}
        title={allowed && !allowed.route_inspection ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.route_inspection", () => routeTableInspection(tableId))}
      >
        table.route_inspection
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.route_validation}
        title={allowed && !allowed.route_validation ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.route_validation", () => routeTableValidation(tableId, null))}
      >
        table.route_validation
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.route_analysis}
        title={allowed && !allowed.route_analysis ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.route_analysis", () => routeTableAnalysis(tableId, null))}
      >
        table.route_analysis
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.archive}
        title={allowed && !allowed.archive ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.archive", () => archiveTable(tableId))}
      >
        table.archive
      </button>
      <button
        type="button"
        disabled={noSelection || !allowed?.restore}
        title={allowed && !allowed.restore ? "Not allowed for this Table's current state" : undefined}
        onClick={() => tableId && run("table.restore", () => restoreTable(tableId))}
      >
        table.restore
      </button>
      <button type="button" disabled={noSelection} onClick={() => tableId && run("table.retry_availability_check", () => retryTableAvailabilityCheck(tableId))}>
        table.retry_availability_check
      </button>
      <button type="button" disabled={noSelection} onClick={() => tableId && run("table.copy_id", () => copyTableId(tableId))}>
        table.copy_id
      </button>
      <p role="status">{message}</p>
    </div>
  );
}
