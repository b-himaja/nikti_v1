import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeDataset, makeTable } from "../../test/fixtures";
import { ExportDatasetDialog } from "./ExportDatasetDialog";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("ExportDatasetDialog", () => {
  it("dispatches export_dataset_as_table with the real args and shows the real returned Table's id/fingerprint", async () => {
    const dataset = makeDataset({ id: "d1", name: "orders" });
    const table = makeTable({ id: "t-exported", table_data_ref: { content_ref: "c", data_fingerprint: "fp-exported", row_count: 3 } });
    invokeMock.mockResolvedValueOnce(table);

    render(<ExportDatasetDialog dataset={dataset} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "dataset.export_as_table" }));

    expect(invokeMock).toHaveBeenCalledWith("export_dataset_as_table", { datasetId: "d1", tableName: "orders" });
    const dl = await screen.findByLabelText("Exported Table");
    expect(dl).toHaveTextContent("t-exported");
    expect(dl).toHaveTextContent("fp-exported");
  });

  it("exporting identical content twice reflects one real Table (same id/fingerprint both times), never implying two independent exports", async () => {
    const dataset = makeDataset({ id: "d1", name: "orders" });
    const table = makeTable({ id: "t-dedup", table_data_ref: { content_ref: "c", data_fingerprint: "fp-dedup", row_count: 3 } });
    invokeMock.mockResolvedValue(table);

    render(<ExportDatasetDialog dataset={dataset} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "dataset.export_as_table" }));
    await screen.findByLabelText("Exported Table");
    fireEvent.click(screen.getByRole("button", { name: "dataset.export_as_table" }));
    const dl = await screen.findByLabelText("Exported Table");

    expect(dl).toHaveTextContent("t-dedup");
    expect(screen.getAllByText("t-dedup")).toHaveLength(1);
  });

  it("Close calls onClose without dispatching any command", () => {
    const onClose = vi.fn();
    render(<ExportDatasetDialog dataset={makeDataset()} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
