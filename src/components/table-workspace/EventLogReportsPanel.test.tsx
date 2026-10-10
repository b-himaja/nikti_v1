import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeEventLog, makeInspectionReport } from "../../test/fixtures";
import { EventLogReportsPanel } from "./EventLogReportsPanel";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("EventLogReportsPanel", () => {
  it("defaults to the Event Log tab, calling table.open_event_log and rendering real entries read-only", async () => {
    invokeMock.mockResolvedValueOnce(makeEventLog());

    render(<EventLogReportsPanel tableId="t1" />);

    expect(await screen.findByText(/imported/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("open_table_event_log", { tableId: "t1" });
    expect(screen.queryByRole("button", { name: /edit|delete/i })).not.toBeInTheDocument();
  });

  it("switching to the Reports tab calls table.open_reports and renders the real AnyReport shape", async () => {
    invokeMock.mockResolvedValueOnce(makeEventLog());
    render(<EventLogReportsPanel tableId="t1" />);
    await screen.findByText(/imported/);

    invokeMock.mockResolvedValueOnce([makeInspectionReport()]);
    fireEvent.click(screen.getByRole("tab", { name: "Reports" }));

    expect(await screen.findByText(/Inspection Report/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("open_table_reports", { tableId: "t1" });
  });

  it("is a single tabbed region — only one tablist for both Event Log and Reports", async () => {
    invokeMock.mockResolvedValueOnce(makeEventLog());
    render(<EventLogReportsPanel tableId="t1" />);
    await screen.findByText(/imported/);
    expect(screen.getAllByRole("tablist")).toHaveLength(1);
  });
});
