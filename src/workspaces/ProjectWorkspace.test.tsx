import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeDataset, makeProject } from "../test/fixtures";
import { useUiStore } from "../store/uiStore";
import { ProjectWorkspace } from "./ProjectWorkspace";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// `cleanup()` MUST run before the store is reset: this screen's
// children (ProjectNavigator) subscribe to store state directly, so
// resetting the store while the previous test's tree is still mounted
// triggers a real re-render and a fresh `listProjects()` call — against
// a mock `afterEach` is about to reset out from under it. Unmount first,
// same as React Testing Library's own automatic cleanup would
// eventually do, just not late enough relative to the store reset here.
afterEach(() => {
  cleanup();
  invokeMock.mockReset();
  useUiStore.setState({ activeWorkspace: "table-repository", activeProjectId: null, handoffDataset: null });
});

// Part B, the specific thing the second status audit flagged as
// unverified: Table Repository and Project Workspace each individually
// tested clean, but nothing had ever proven the hand-off between them
// actually lands on the correct, newly-created Dataset once mounted.
// This test starts from exactly the state Toolbar's
// table.load_into_project/table.create_project_from_table success path
// leaves the store in (see Toolbar.test.tsx for the other half — that
// the store gets set correctly), then mounts the real ProjectWorkspace
// on top of it and confirms it reads and displays that exact Dataset.
describe("ProjectWorkspace — Part B hand-off", () => {
  it("on mount, consumes the store's handoffDataset and renders it in DatasetWorkspace — not a placeholder, not empty", async () => {
    const dataset = makeDataset({ id: "d-handoff", project_id: "p1", name: "orders-from-handoff" });
    const project = makeProject({ id: "p1", name: "Orders Project", dataset_ids: ["d-handoff"] });
    useUiStore.setState({ activeWorkspace: "project", activeProjectId: "p1", handoffDataset: dataset });

    invokeMock.mockImplementation((command: string) => {
      if (command === "open_project") return Promise.resolve(project);
      if (command === "list_projects") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    render(<ProjectWorkspace />);

    // The exact handed-off Dataset's real name, not a guessed/default
    // one and not the old "(placeholder)" heading.
    expect(await screen.findByRole("heading", { name: "orders-from-handoff", level: 2 })).toBeInTheDocument();
    expect(screen.queryByText(/placeholder/i)).not.toBeInTheDocument();

    // One-shot: consumed immediately, never re-readable afterward.
    expect(useUiStore.getState().handoffDataset).toBeNull();

    // The real Project this activeProjectId resolves to was also
    // fetched for real (open_project), not left null.
    expect(await screen.findByText("Orders Project")).toBeInTheDocument();

    // Let ProjectNavigator's own independent list_projects effect chain
    // settle too before the test ends.
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_projects", {}));
  });

  it("with no hand-off pending (e.g. a direct navigation), shows the real empty state rather than crashing or fabricating a Dataset", async () => {
    invokeMock.mockResolvedValue([]);
    render(<ProjectWorkspace />);
    expect(screen.getByText(/No Dataset selected/)).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_projects", {}));
  });
});
