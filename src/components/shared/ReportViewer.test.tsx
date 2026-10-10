import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeComparisonReport, makeLineageReport, makeReportWithContent } from "../../test/fixtures";
import { ReportViewer } from "./ReportViewer";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("ReportViewer", () => {
  it("dispatches open_report with the real report_id and renders the real returned metadata and content, read-only", async () => {
    const result = makeReportWithContent();
    invokeMock.mockResolvedValueOnce(result);

    render(<ReportViewer reportId="report-1" onClose={() => {}} />);

    expect(invokeMock).toHaveBeenCalledWith("open_report", { reportId: "report-1" });
    expect(await screen.findByText("Inspection Report")).toBeInTheDocument();
    expect(screen.getByText("Available")).toBeInTheDocument();
    expect(screen.getByText("measurements")).toBeInTheDocument();
    expect(screen.getByText(/"row_count": 3/)).toBeInTheDocument();
    // No edit/delete/reorder affordance anywhere on evidence (INV-021).
    expect(screen.queryByRole("button", { name: /edit|delete|save/i })).not.toBeInTheDocument();
    expect(screen.queryAllByRole("textbox")).toHaveLength(0);
  });

  // Audit #3's finding: Comparison's diff content was illegible as a
  // raw JSON blob. This proves the fix — a real diff, real values, no
  // JSON dump of the outcome/diff structure anywhere in the rendered
  // output.
  it("renders a Comparison report's diff as a readable before/after table, never a raw JSON dump", async () => {
    const result = makeReportWithContent({
      report: makeComparisonReport(),
      content: {
        report_a_id: "44444444-4444-4444-8444-444444444444",
        report_b_id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        report_kind: "Inspection",
        outcome: { Diffed: [{ Changed: { path: "measurements.row_count", before: 3, after: 5 } }, { Added: { path: "measurements.new_field", value: "x" } }] },
      },
    });
    invokeMock.mockResolvedValueOnce(result);

    render(<ReportViewer reportId="report-comparison" onClose={() => {}} />);

    expect(await screen.findByRole("table", { name: "Comparison diff" })).toBeInTheDocument();
    expect(screen.getByText("measurements.row_count")).toBeInTheDocument();
    expect(screen.getByText("Changed")).toBeInTheDocument();
    expect(screen.getByText("Added")).toBeInTheDocument();
    expect(screen.getByText("measurements.new_field")).toBeInTheDocument();
    // Never a raw dump of the `outcome`/`Diffed` wrapper structure itself.
    expect(screen.queryByText(/"Diffed"/)).not.toBeInTheDocument();
    expect(screen.queryByText("outcome")).not.toBeInTheDocument();
  });

  it("renders a Comparison report's schema-version mismatch as a plain sentence, not a raw JSON dump", async () => {
    const result = makeReportWithContent({
      report: makeComparisonReport(),
      content: {
        report_a_id: "44444444-4444-4444-8444-444444444444",
        report_b_id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        report_kind: "Inspection",
        outcome: { SchemaVersionMismatch: { report_a_schema_version: "1.0", report_b_schema_version: "2.0" } },
      },
    });
    invokeMock.mockResolvedValueOnce(result);

    render(<ReportViewer reportId="report-comparison-mismatch" onClose={() => {}} />);

    expect(await screen.findByText(/Schema versions do not match/)).toBeInTheDocument();
    expect(screen.getByText("1.0")).toBeInTheDocument();
    expect(screen.getByText("2.0")).toBeInTheDocument();
    expect(screen.queryByText(/"SchemaVersionMismatch"/)).not.toBeInTheDocument();
  });

  // Audit #3's other finding: Lineage's node/edge graph was illegible as
  // a raw JSON blob. Proves a real edge (including `DatasetDuplicated`,
  // called out explicitly in the task) renders as a readable row.
  it("renders a Lineage report's edges as a readable source->target list, never a raw JSON dump", async () => {
    const result = makeReportWithContent({
      report: makeLineageReport(),
      content: {
        nodes: [{ ImportedTable: "11111111-1111-4111-8111-111111111111" }, { Dataset: "66666666-6666-4666-8666-666666666666" }],
        edges: [
          { LoadIntoProject: { table_id: "11111111-1111-4111-8111-111111111111", dataset_id: "66666666-6666-4666-8666-666666666666" } },
          { DatasetDuplicated: { from_dataset_id: "66666666-6666-4666-8666-666666666666", to_dataset_id: "77777777-7777-4777-8777-777777777777" } },
        ],
        upstream_count: 2,
        downstream_count: null,
        archived_or_purged: [],
        reproducible: true,
      },
    });
    invokeMock.mockResolvedValueOnce(result);

    render(<ReportViewer reportId="report-lineage" onClose={() => {}} />);

    expect(await screen.findByRole("table", { name: "Lineage edges" })).toBeInTheDocument();
    expect(screen.getByText("LoadIntoProject")).toBeInTheDocument();
    expect(screen.getByText("DatasetDuplicated")).toBeInTheDocument();
    expect(screen.getByText("true", { exact: false })).toBeInTheDocument(); // reproducible
    // Never a raw dump of the `edges` array itself.
    expect(screen.queryByText(/"LoadIntoProject"/)).not.toBeInTheDocument();
    expect(screen.queryByText(/"DatasetDuplicated"/)).not.toBeInTheDocument();
  });

  it("shows the real error, not a fabricated fallback, when open_report rejects", async () => {
    invokeMock.mockRejectedValueOnce(new Error("not found"));
    render(<ReportViewer reportId="missing" onClose={() => {}} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("not found");
  });

  it("Close calls onClose", () => {
    const onClose = vi.fn();
    invokeMock.mockResolvedValueOnce(makeReportWithContent());
    render(<ReportViewer reportId="report-1" onClose={onClose} />);
    screen.getByRole("button", { name: "Close" }).click();
    expect(onClose).toHaveBeenCalled();
  });
});
