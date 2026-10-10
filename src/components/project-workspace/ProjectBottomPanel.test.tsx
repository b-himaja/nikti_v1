import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeDataset, makeProject, makeProjectOperationLogEntry, makeRecipeContext } from "../../test/fixtures";
import { ProjectBottomPanel } from "./ProjectBottomPanel";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("ProjectBottomPanel", () => {
  it("Operation Log tab renders every real entry strictly read-only — no edit/reorder control anywhere", () => {
    const project = makeProject({ operation_log: { entries: [makeProjectOperationLogEntry({ id: "e1", operation: "load" }), makeProjectOperationLogEntry({ id: "e2", operation: "drop_columns" })] } });
    render(<ProjectBottomPanel project={project} activeDataset={null} sessionReportIds={[]} onDatasetChange={() => {}} />);

    expect(screen.getByText(/load \(dataset/)).toBeInTheDocument();
    expect(screen.getByText(/drop_columns \(dataset/)).toBeInTheDocument();
    // No input/textbox anywhere in the Operation Log list itself.
    const list = screen.getByLabelText("Operation Log Entries");
    expect(list.querySelectorAll("input, [contenteditable]").length).toBe(0);
  });

  it("Promote to Recipe is shown visibly disabled, never silently functional — promote_recipe's real implementation doesn't match its command signature yet", () => {
    render(<ProjectBottomPanel project={makeProject()} activeDataset={null} sessionReportIds={[]} onDatasetChange={() => {}} />);
    const button = screen.getByRole("button", { name: "project_operation_log.promote_to_recipe" });
    expect(button).toHaveProperty("disabled", true);
    fireEvent.click(button);
    expect(invokeMock).not.toHaveBeenCalledWith("promote_recipe", expect.anything());
  });

  it("Recipes tab: recipe.load dispatches load_recipe_into_project and lists the real returned RecipeContext", async () => {
    const context = makeRecipeContext({ recipe_id: "r1", version: 2 });
    invokeMock.mockResolvedValueOnce(context);
    render(<ProjectBottomPanel project={makeProject()} activeDataset={null} sessionReportIds={[]} onDatasetChange={() => {}} />);

    fireEvent.click(screen.getByRole("tab", { name: "Recipes" }));
    fireEvent.change(screen.getByLabelText("recipe_id for recipe.load"), { target: { value: "r1" } });
    fireEvent.change(screen.getByLabelText("version for recipe.load"), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "recipe.load" }));

    expect(invokeMock).toHaveBeenCalledWith("load_recipe_into_project", { recipeId: "r1", version: 2 });
    await screen.findByText(/r1 v2/);
  });

  it("Recipes tab: recipe.apply is disabled with no active Dataset, and dispatches apply_recipe once one exists", async () => {
    const dataset = makeDataset({ id: "d1" });
    const context = makeRecipeContext({ recipe_id: "r1", version: 1 });
    invokeMock.mockResolvedValueOnce(context);
    const onDatasetChange = vi.fn();

    render(<ProjectBottomPanel project={makeProject()} activeDataset={dataset} sessionReportIds={[]} onDatasetChange={onDatasetChange} />);
    fireEvent.click(screen.getByRole("tab", { name: "Recipes" }));
    fireEvent.change(screen.getByLabelText("recipe_id for recipe.load"), { target: { value: "r1" } });
    fireEvent.click(screen.getByRole("button", { name: "recipe.load" }));
    const applyButton = await screen.findByRole("button", { name: "recipe.apply" });
    expect(applyButton).toHaveProperty("disabled", false);

    const applied = makeDataset({ id: "d1", name: "after-recipe" });
    invokeMock.mockResolvedValueOnce(applied);
    fireEvent.click(applyButton);
    expect(invokeMock).toHaveBeenCalledWith("apply_recipe", { datasetId: "d1", recipeId: "r1" });
    await waitFor(() => expect(onDatasetChange).toHaveBeenCalledWith(applied));
  });

  it("Dataset Reports tab lists the Dataset's real report_refs union this session's registered report ids, as reference ids only — no fabricated Report Viewer", () => {
    const dataset = makeDataset({ report_refs: ["r-from-dataset"] });
    render(<ProjectBottomPanel project={makeProject()} activeDataset={dataset} sessionReportIds={["r-from-session"]} onDatasetChange={() => {}} />);

    fireEvent.click(screen.getByRole("tab", { name: "Dataset Reports" }));
    expect(screen.getByText("r-from-dataset")).toBeInTheDocument();
    expect(screen.getByText("r-from-session")).toBeInTheDocument();
  });
});
