import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeProject } from "../../test/fixtures";
import { ProjectNavigator } from "./ProjectNavigator";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("ProjectNavigator", () => {
  it("dispatches list_projects and renders the real response, selecting a project calls onSelectProject", async () => {
    invokeMock.mockResolvedValueOnce([{ project_id: "p1", name: "Orders Project", dataset_count: 2, created_at: 1_000 }]);
    const onSelectProject = vi.fn();

    render(<ProjectNavigator activeProjectId={null} project={null} activeDatasetId={null} onSelectProject={onSelectProject} />);

    expect(invokeMock).toHaveBeenCalledWith("list_projects", {});
    const button = await screen.findByRole("button", { name: "Orders Project" });
    fireEvent.click(button);
    expect(onSelectProject).toHaveBeenCalledWith("p1");
  });

  it("renders the open Project's real dataset_ids, never a second reconstructed list", () => {
    invokeMock.mockResolvedValueOnce([]);
    const project = makeProject({ dataset_ids: ["d1", "d2"] });

    render(<ProjectNavigator activeProjectId={project.id} project={project} activeDatasetId="d1" onSelectProject={() => {}} />);

    expect(screen.getByText("d1")).toBeInTheDocument();
    expect(screen.getByText("d2")).toBeInTheDocument();
  });

  it("Recipe References and Supporting Objects are shown as not-yet-available, never fabricated data", () => {
    invokeMock.mockResolvedValueOnce([]);
    render(<ProjectNavigator activeProjectId={null} project={null} activeDatasetId={null} onSelectProject={() => {}} />);

    expect(screen.getByRole("heading", { name: "Recipe References" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Supporting Objects" })).toBeInTheDocument();
    expect(screen.getAllByText(/Not available/)).toHaveLength(2);
  });
});
