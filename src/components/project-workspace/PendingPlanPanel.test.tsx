import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeDataset, makePendingPlan } from "../../test/fixtures";
import { PendingPlanPanel } from "./PendingPlanPanel";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("PendingPlanPanel", () => {
  it("pending_plan.save dispatches save_pending_plan with the real args and renders only fields the real PendingTransformationPlan struct has (no fabricated plan_state/definition_status)", async () => {
    const dataset = makeDataset({ id: "d1", schema: { columns: [{ name: "amount", data_type: "Float64" }] } });
    const saved = makePendingPlan({ id: "plan-1", name: "my plan" });
    invokeMock.mockResolvedValueOnce(saved);

    render(<PendingPlanPanel projectId="p1" dataset={dataset} onDatasetChange={() => {}} />);
    fireEvent.change(screen.getByLabelText("name for pending_plan.save"), { target: { value: "my plan" } });
    fireEvent.change(screen.getByLabelText("column for pending_plan.save"), { target: { value: "amount" } });
    fireEvent.click(screen.getByRole("button", { name: "pending_plan.save" }));

    expect(invokeMock).toHaveBeenCalledWith("save_pending_plan", { projectId: "p1", datasetId: "d1", name: "my plan", operations: [{ DropColumns: { columns: ["amount"] } }] });
    const dl = await screen.findByLabelText("Loaded Pending Plan");
    expect(dl).toHaveTextContent("plan-1");
    expect(dl).not.toHaveTextContent("plan_state");
    expect(dl).not.toHaveTextContent("definition_status");
  });

  it("pending_plan.load dispatches load_pending_plan, then pending_plan.run dispatches run_pending_plan and reports the real returned Dataset", async () => {
    const plan = makePendingPlan({ id: "plan-1" });
    const updated = makeDataset({ id: "d2", name: "after-plan" });
    invokeMock.mockResolvedValueOnce(plan);
    const onDatasetChange = vi.fn();

    render(<PendingPlanPanel projectId="p1" dataset={makeDataset()} onDatasetChange={onDatasetChange} />);
    fireEvent.change(screen.getByLabelText("plan_id for pending_plan.load"), { target: { value: "plan-1" } });
    fireEvent.click(screen.getByRole("button", { name: "pending_plan.load" }));
    await screen.findByLabelText("Loaded Pending Plan");
    expect(invokeMock).toHaveBeenCalledWith("load_pending_plan", { planId: "plan-1" });

    invokeMock.mockResolvedValueOnce(updated);
    fireEvent.click(screen.getByRole("button", { name: "pending_plan.run" }));
    expect(invokeMock).toHaveBeenCalledWith("run_pending_plan", { planId: "plan-1" });
    await waitFor(() => expect(onDatasetChange).toHaveBeenCalledWith(updated));
  });

  it("pending_plan.run is disabled until a plan is loaded or saved", () => {
    render(<PendingPlanPanel projectId="p1" dataset={makeDataset()} onDatasetChange={() => {}} />);
    expect(screen.getByRole("button", { name: "pending_plan.run" })).toHaveProperty("disabled", true);
  });
});
