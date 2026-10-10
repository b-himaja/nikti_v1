import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeDataset, makeRegistrationOutcome } from "../../test/fixtures";
import { DatasetWorkspace } from "./DatasetWorkspace";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("DatasetWorkspace", () => {
  it("shows an honest empty state, never a fabricated grid, when no Dataset is selected", () => {
    render(<DatasetWorkspace dataset={null} onDatasetChange={() => {}} onOpenExportDialog={() => {}} onReportRegistered={() => {}} />);
    expect(screen.getByText(/No Dataset selected/)).toBeInTheDocument();
  });

  it("dataset.transform.drop_column dispatches transform_dataset with the real command name/args and calls onDatasetChange with the real returned Dataset", async () => {
    const dataset = makeDataset({ schema: { columns: [{ name: "id", data_type: "Int64" }, { name: "amount", data_type: "Float64" }] } });
    const updated = makeDataset({ id: dataset.id, schema: { columns: [{ name: "id", data_type: "Int64" }] } });
    invokeMock.mockResolvedValueOnce(updated);
    const onDatasetChange = vi.fn();

    render(<DatasetWorkspace dataset={dataset} onDatasetChange={onDatasetChange} onOpenExportDialog={() => {}} onReportRegistered={() => {}} />);
    fireEvent.change(screen.getByLabelText("Column for dataset.transform.drop_column"), { target: { value: "amount" } });
    fireEvent.click(screen.getByRole("button", { name: "dataset.transform.drop_column" }));

    expect(invokeMock).toHaveBeenCalledWith("transform_dataset", { datasetId: dataset.id, operation: { DropColumns: { columns: ["amount"] } } });
    await screen.findByText(/dataset\.transform\.drop_column/);
    expect(onDatasetChange).toHaveBeenCalledWith(updated);
  });

  it("dataset.run_validation dispatches run_validation_on_dataset with the entered ruleset id/version and reports the real returned report id, never applying a transform", async () => {
    const dataset = makeDataset();
    const outcome = makeRegistrationOutcome({ report_id: "report-1" });
    invokeMock.mockResolvedValueOnce(outcome);
    const onReportRegistered = vi.fn();
    const onDatasetChange = vi.fn();

    render(<DatasetWorkspace dataset={dataset} onDatasetChange={onDatasetChange} onOpenExportDialog={() => {}} onReportRegistered={onReportRegistered} />);
    fireEvent.change(screen.getByLabelText("ruleset_id for dataset.run_validation"), { target: { value: "r1" } });
    fireEvent.change(screen.getByLabelText("ruleset_version_id for dataset.run_validation"), { target: { value: "v1" } });
    fireEvent.click(screen.getByRole("button", { name: "dataset.run_validation" }));

    expect(invokeMock).toHaveBeenCalledWith("run_validation_on_dataset", { datasetId: dataset.id, rulesetId: "r1", rulesetVersionId: "v1" });
    await waitFor(() => expect(onReportRegistered).toHaveBeenCalledWith("report-1"));
    expect(onDatasetChange).not.toHaveBeenCalled();
  });

  it("dataset.export_as_table opens the export dialog rather than exporting inline — no direct export_dataset_as_table call from this toolbar", () => {
    const onOpenExportDialog = vi.fn();
    render(<DatasetWorkspace dataset={makeDataset()} onDatasetChange={() => {}} onOpenExportDialog={onOpenExportDialog} onReportRegistered={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "dataset.export_as_table" }));
    expect(onOpenExportDialog).toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("export_dataset_as_table", expect.anything());
  });

  it("run_validation/run_analysis buttons are disabled until both a ruleset id and version id are entered — never dispatchable with empty identifiers", () => {
    render(<DatasetWorkspace dataset={makeDataset()} onDatasetChange={() => {}} onOpenExportDialog={() => {}} onReportRegistered={() => {}} />);
    expect(screen.getByRole("button", { name: "dataset.run_validation" })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: "dataset.run_analysis" })).toHaveProperty("disabled", true);
  });
});
