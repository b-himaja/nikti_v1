import { useEffect, useState } from "react";

import { openReport } from "../../utils/ipc";
import type { AnyReport, AnyReportKind, ReportWithContent } from "../../utils/ipc";

interface Props {
  reportId: string;
  onClose: () => void;
}

function reportEntry(report: AnyReport): { kind: AnyReportKind; title: string; state: string; engine: string; createdBy: string; createdAt: number } {
  const [kind, content] = Object.entries(report)[0] as [
    AnyReportKind,
    { report_title: string; report_state: string; generated_by_engine: string; created_by: string; created_at: number },
  ];
  return { kind, title: content.report_title, state: content.report_state, engine: content.generated_by_engine, createdBy: content.created_by, createdAt: content.created_at };
}

// The generic, by-top-level-key rendering every report kind used before
// this task — still exactly what Inspection/Validation/Analysis/Audit
// get (Part B.4: their rendering path is untouched). Content genuinely
// varies in shape per kind (rule_results for Validation, procedure_results
// for Analysis, and so on — see `open_report`'s own doc comment), and
// this remains the honest way to render a body whose exact shape isn't
// assumed — it's just no longer the ONLY rendering path, now that
// Comparison and Lineage have shapes specific enough to read as JSON
// blobs (status audit 3's finding).
function GenericContentView({ contentObj }: { contentObj: Record<string, unknown> }) {
  return (
    <>
      {Object.entries(contentObj).map(([key, value]) => (
        <div key={key}>
          <h4>{key}</h4>
          <pre>{JSON.stringify(value, null, 2)}</pre>
        </div>
      ))}
      {Object.keys(contentObj).length === 0 && <p>(no content body)</p>}
    </>
  );
}

// Mirrors `nikti_core::types::report_comparison::{ComparisonContent,
// ComparisonOutcome, DiffEntry}` exactly (read directly, not invented) —
// serde's default externally-tagged representation for both enums.
interface ComparisonDiffEntryShape {
  Added?: { path: string; value: unknown };
  Removed?: { path: string; value: unknown };
  Changed?: { path: string; before: unknown; after: unknown };
}
interface ComparisonContentShape {
  report_a_id: string;
  report_b_id: string;
  report_kind: string;
  outcome: { Diffed: ComparisonDiffEntryShape[] } | { SchemaVersionMismatch: { report_a_schema_version: string; report_b_schema_version: string } };
}

function ComparisonContentView({ contentObj }: { contentObj: Record<string, unknown> }) {
  const content = contentObj as unknown as Partial<ComparisonContentShape>;
  if (!content.outcome) {
    return <p>(no comparison outcome)</p>;
  }
  if ("SchemaVersionMismatch" in content.outcome) {
    const { report_a_schema_version, report_b_schema_version } = content.outcome.SchemaVersionMismatch;
    return (
      <p>
        Schema versions do not match — report A is <code>{report_a_schema_version}</code>, report B is <code>{report_b_schema_version}</code>. No diff was computed; comparing incompatible
        shapes would not be honest.
      </p>
    );
  }
  const entries = content.outcome.Diffed;
  if (entries.length === 0) {
    return <p>No differences — the two reports&apos; content is identical.</p>;
  }
  return (
    <table aria-label="Comparison diff">
      <thead>
        <tr>
          <th>Change</th>
          <th>Path</th>
          <th>Before</th>
          <th>After</th>
        </tr>
      </thead>
      <tbody>
        {entries.map((entry, i) => {
          if (entry.Added) {
            return (
              <tr key={i}>
                <td>Added</td>
                <td>{entry.Added.path}</td>
                <td>—</td>
                <td>
                  <pre>{JSON.stringify(entry.Added.value)}</pre>
                </td>
              </tr>
            );
          }
          if (entry.Removed) {
            return (
              <tr key={i}>
                <td>Removed</td>
                <td>{entry.Removed.path}</td>
                <td>
                  <pre>{JSON.stringify(entry.Removed.value)}</pre>
                </td>
                <td>—</td>
              </tr>
            );
          }
          const changed = entry.Changed!;
          return (
            <tr key={i}>
              <td>Changed</td>
              <td>{changed.path}</td>
              <td>
                <pre>{JSON.stringify(changed.before)}</pre>
              </td>
              <td>
                <pre>{JSON.stringify(changed.after)}</pre>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// Mirrors `nikti_core::types::report_lineage::{LineageContent,
// LineageEdge}` exactly (read directly) — struct-like enum variants
// serialize as `{"VariantName": {field: value, ...}}`.
interface LineageEdgeShape {
  LoadIntoProject?: { table_id: string; dataset_id: string };
  TransformationRun?: { dataset_id: string; operation_log_entry_id: string; operation: string };
  RecipeApplied?: { from_dataset_id: string; to_dataset_id: string; recipe_id: string };
  DatasetDuplicated?: { from_dataset_id: string; to_dataset_id: string };
  GeneratedReport?: { source: unknown; report_id: string };
  ExportAsTable?: { dataset_id: string; table_id: string };
}
interface LineageContentShape {
  nodes: unknown[];
  edges: LineageEdgeShape[];
  upstream_count: number;
  downstream_count: number | null;
  archived_or_purged: unknown[];
  reproducible: boolean;
}

function describeLineageEdge(edge: LineageEdgeShape): { type: string; from: string; to: string } {
  if (edge.LoadIntoProject) return { type: "LoadIntoProject", from: `table:${edge.LoadIntoProject.table_id}`, to: `dataset:${edge.LoadIntoProject.dataset_id}` };
  if (edge.TransformationRun) return { type: `TransformationRun (${edge.TransformationRun.operation})`, from: `dataset:${edge.TransformationRun.dataset_id}`, to: `dataset:${edge.TransformationRun.dataset_id}` };
  if (edge.RecipeApplied) return { type: "RecipeApplied", from: `dataset:${edge.RecipeApplied.from_dataset_id}`, to: `dataset:${edge.RecipeApplied.to_dataset_id}` };
  if (edge.DatasetDuplicated) return { type: "DatasetDuplicated", from: `dataset:${edge.DatasetDuplicated.from_dataset_id}`, to: `dataset:${edge.DatasetDuplicated.to_dataset_id}` };
  if (edge.GeneratedReport) return { type: "GeneratedReport", from: JSON.stringify(edge.GeneratedReport.source), to: `report:${edge.GeneratedReport.report_id}` };
  if (edge.ExportAsTable) return { type: "ExportAsTable", from: `dataset:${edge.ExportAsTable.dataset_id}`, to: `table:${edge.ExportAsTable.table_id}` };
  return { type: "(unrecognized edge)", from: "?", to: "?" };
}

function LineageContentView({ contentObj }: { contentObj: Record<string, unknown> }) {
  const content = contentObj as unknown as Partial<LineageContentShape>;
  const nodes = content.nodes ?? [];
  const edges = content.edges ?? [];
  return (
    <>
      <dl aria-label="Lineage Summary">
        <dt>reproducible</dt>
        <dd>{String(content.reproducible ?? false)}</dd>
        <dt>upstream_count</dt>
        <dd>{content.upstream_count ?? 0}</dd>
        <dt>downstream_count</dt>
        <dd>{content.downstream_count ?? "not checked (forward traversal out of scope)"}</dd>
        <dt>archived_or_purged</dt>
        <dd>{(content.archived_or_purged?.length ?? 0) === 0 ? "none" : JSON.stringify(content.archived_or_purged)}</dd>
      </dl>
      <h4>Nodes ({nodes.length})</h4>
      <ul>
        {nodes.map((node, i) => (
          <li key={i}>{JSON.stringify(node)}</li>
        ))}
      </ul>
      <h4>Edges ({edges.length})</h4>
      {edges.length === 0 ? (
        <p>(no edges)</p>
      ) : (
        <table aria-label="Lineage edges">
          <thead>
            <tr>
              <th>Type</th>
              <th>From</th>
              <th>To</th>
            </tr>
          </thead>
          <tbody>
            {edges.map((edge, i) => {
              const d = describeLineageEdge(edge);
              return (
                <tr key={i}>
                  <td>{d.type}</td>
                  <td>{d.from}</td>
                  <td>{d.to}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </>
  );
}

function assertNeverReportKind(kind: never): never {
  throw new Error(`ReportViewer: unhandled report kind ${JSON.stringify(kind)} — a new ReportKind variant needs a render case added here`);
}

// The kind-specific render switch (Part B of the Audit #3 cleanup task).
// Switches on `AnyReportKind` — the same compiler-derived key union
// `ipc.ts` already ties to `AnyReport`'s real variant keys — so a 7th
// report kind added later fails to compile here (the `default` branch's
// `kind` stops narrowing to `never`) rather than silently falling
// through to generic rendering unnoticed, the same discipline the
// Rust-side exhaustiveness tests apply.
function renderReportContent(kind: AnyReportKind, contentObj: Record<string, unknown>) {
  switch (kind) {
    case "Comparison":
      return <ComparisonContentView contentObj={contentObj} />;
    case "Lineage":
      return <LineageContentView contentObj={contentObj} />;
    case "Inspection":
    case "Validation":
    case "Analysis":
    case "Audit":
      return <GenericContentView contentObj={contentObj} />;
    default:
      return assertNeverReportKind(kind);
  }
}

// The one, shared Report Viewer — reused verbatim by Table Repository's
// Reports tab and Project Workspace's Dataset Reports tab (and, once it
// exists, the Report Repository workspace). Per INV-022/023 this is a
// single component instance imported by every caller, never a
// per-workspace reimplementation.
export function ReportViewer({ reportId, onClose }: Props) {
  const [result, setResult] = useState<ReportWithContent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setResult(null);
    setError(null);
    openReport(reportId)
      .then((r) => {
        if (!cancelled) setResult(r);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [reportId]);

  return (
    <div role="dialog" aria-label="Report Viewer">
      <button type="button" onClick={onClose}>
        Close
      </button>

      {error && <p role="alert">{error}</p>}
      {!result && !error && <p>Loading…</p>}

      {result &&
        (() => {
          const entry = reportEntry(result.report);
          const contentObj = typeof result.content === "object" && result.content !== null ? (result.content as Record<string, unknown>) : {};
          return (
            <>
              <h2>{entry.title}</h2>
              <dl aria-label="Report Metadata">
                <dt>report_type</dt>
                <dd>{entry.kind}</dd>
                <dt>report_state</dt>
                <dd>{entry.state}</dd>
                <dt>generated_by_engine</dt>
                <dd>{entry.engine}</dd>
                <dt>created_by</dt>
                <dd>{entry.createdBy}</dd>
                <dt>created_at</dt>
                <dd>{entry.createdAt}</dd>
              </dl>

              <section aria-label="Report Content">
                <h3>Content</h3>
                {renderReportContent(entry.kind, contentObj)}
              </section>
            </>
          );
        })()}
    </div>
  );
}
