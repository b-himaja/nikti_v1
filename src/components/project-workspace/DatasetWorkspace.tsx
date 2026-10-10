import { useState } from "react";

import { duplicateDataset, runAnalysisOnDataset, runValidationOnDataset, transformDataset } from "../../utils/ipc";
import type { Dataset, RegistrationOutcome } from "../../utils/ipc";

interface Props {
  dataset: Dataset | null;
  onDatasetChange: (dataset: Dataset) => void;
  onOpenExportDialog: () => void;
  onReportRegistered: (reportId: string) => void;
}

function reportIdFromOutcome(outcome: RegistrationOutcome): string {
  return Object.values(outcome)[0].report_id;
}

// Center panel — the active Dataset plus a Transformation Toolbar.
//
// This is NOT a data grid. No command anywhere returns row-level cell
// content for a Dataset (or a Table) — `Dataset` carries only
// `schema`/`row_count`/`content_ref` (a Parquet file reference), never
// materialized rows, and no `read_dataset_content`/preview command
// exists in the command layer. Building a client-side Parquet reader
// would be new domain logic this task is explicitly scoped not to add
// (INV-022). So this view shows real schema/metadata only, honestly
// labeled — not a fabricated or placeholder grid pretending to be one.
//
// Every transform button dispatches exactly one canonical command, per
// INV-013: transformations target this Dataset's id, never a Table.
// Validation/Analysis need a ruleset id + a specific ruleset version id
// (never "the latest" — same discipline as Recipe versions); there is
// no ruleset-browsing/listing command in this command layer to populate
// a picker from, so these are plain text inputs, not a fabricated
// dropdown of invented options.
export function DatasetWorkspace({ dataset, onDatasetChange, onOpenExportDialog, onReportRegistered }: Props) {
  const [message, setMessage] = useState("(no command run yet)");
  const [duplicateName, setDuplicateName] = useState("");
  const [dropColumn, setDropColumn] = useState("");
  const [validationRulesetId, setValidationRulesetId] = useState("");
  const [validationVersionId, setValidationVersionId] = useState("");
  const [analysisRulesetId, setAnalysisRulesetId] = useState("");
  const [analysisVersionId, setAnalysisVersionId] = useState("");

  async function run(label: string, fn: () => Promise<unknown>) {
    try {
      const result = await fn();
      setMessage(`${label}: ${JSON.stringify(result)}`);
      return result;
    } catch (err) {
      setMessage(`${label}: ${String(err)}`);
      return null;
    }
  }

  if (!dataset) {
    return <section aria-label="Dataset Workspace">No Dataset selected. Load one from the Table Repository, or select one from the Project Navigator.</section>;
  }

  return (
    <section aria-label="Dataset Workspace">
      <h2>{dataset.name}</h2>

      <section aria-label="Dataset Metadata">
        <dl>
          <dt>id</dt>
          <dd>{dataset.id}</dd>
          <dt>row_count</dt>
          <dd>{dataset.row_count}</dd>
          <dt>content_fingerprint</dt>
          <dd>{dataset.content_fingerprint}</dd>
          <dt>columns</dt>
          <dd>
            <ul>
              {dataset.schema.columns.map((c) => (
                <li key={c.name}>
                  {c.name}: {c.data_type}
                </li>
              ))}
            </ul>
          </dd>
        </dl>
      </section>

      <div role="toolbar" aria-label="Transformation Toolbar">
        <label>
          New name for duplicate
          <input aria-label="Name for dataset.duplicate" value={duplicateName} onChange={(e) => setDuplicateName(e.target.value)} />
        </label>
        <button
          type="button"
          disabled={!duplicateName}
          onClick={async () => {
            const result = await run("dataset.duplicate", () => duplicateDataset(dataset.id, duplicateName));
            if (result) onDatasetChange(result as Dataset);
          }}
        >
          dataset.duplicate
        </button>

        <label>
          Column to drop
          <select aria-label="Column for dataset.transform.drop_column" value={dropColumn} onChange={(e) => setDropColumn(e.target.value)}>
            <option value="">(select a column)</option>
            {dataset.schema.columns.map((c) => (
              <option key={c.name} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          disabled={!dropColumn}
          onClick={async () => {
            const result = await run("dataset.transform.drop_column", () => transformDataset(dataset.id, { DropColumns: { columns: [dropColumn] } }));
            if (result) onDatasetChange(result as Dataset);
          }}
        >
          dataset.transform.drop_column
        </button>

        <button
          type="button"
          onClick={async () => {
            const result = await run("dataset.transform.generate_row_number", () => transformDataset(dataset.id, { GenerateRowNumber: { output_col: null, start: 1 } }));
            if (result) onDatasetChange(result as Dataset);
          }}
        >
          dataset.transform.generate_row_number
        </button>

        <button type="button" onClick={onOpenExportDialog}>
          dataset.export_as_table
        </button>
      </div>

      <div role="toolbar" aria-label="Validation and Analysis Toolbar">
        <fieldset>
          <legend>dataset.run_validation</legend>
          <label>
            ruleset_id
            <input aria-label="ruleset_id for dataset.run_validation" value={validationRulesetId} onChange={(e) => setValidationRulesetId(e.target.value)} />
          </label>
          <label>
            ruleset_version_id
            <input aria-label="ruleset_version_id for dataset.run_validation" value={validationVersionId} onChange={(e) => setValidationVersionId(e.target.value)} />
          </label>
          <button
            type="button"
            disabled={!validationRulesetId || !validationVersionId}
            onClick={async () => {
              const outcome = await run("dataset.run_validation", () => runValidationOnDataset(dataset.id, validationRulesetId, validationVersionId));
              if (outcome) onReportRegistered(reportIdFromOutcome(outcome as RegistrationOutcome));
            }}
          >
            dataset.run_validation
          </button>
        </fieldset>

        <fieldset>
          <legend>dataset.run_analysis</legend>
          <label>
            ruleset_id
            <input aria-label="ruleset_id for dataset.run_analysis" value={analysisRulesetId} onChange={(e) => setAnalysisRulesetId(e.target.value)} />
          </label>
          <label>
            ruleset_version_id
            <input aria-label="ruleset_version_id for dataset.run_analysis" value={analysisVersionId} onChange={(e) => setAnalysisVersionId(e.target.value)} />
          </label>
          <button
            type="button"
            disabled={!analysisRulesetId || !analysisVersionId}
            onClick={async () => {
              const outcome = await run("dataset.run_analysis", () => runAnalysisOnDataset(dataset.id, analysisRulesetId, analysisVersionId));
              if (outcome) onReportRegistered(reportIdFromOutcome(outcome as RegistrationOutcome));
            }}
          >
            dataset.run_analysis
          </button>
        </fieldset>
      </div>

      <p role="status">{message}</p>
    </section>
  );
}
