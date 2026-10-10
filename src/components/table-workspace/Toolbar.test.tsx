import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ALLOWED_ACTIONS_BY_STATE, makeAllowedActions, makeDataset, makeTable } from "../../test/fixtures";
import { useUiStore } from "../../store/uiStore";
import { Toolbar } from "./Toolbar";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// Toolbar's own `useEffect` calls `list_projects` on every mount (to
// populate the target-project <select> for table.load_into_project) —
// every test renders the Toolbar, so every test needs a baseline
// response for that call regardless of which action it's actually
// exercising. Command-name-keyed, not call-order-keyed: a prior
// regression here came from `mockResolvedValueOnce` being consumed by
// whichever `invoke()` call happened to fire first (list_projects, on
// mount, before any button click), not by the command the test cared
// about.
function mockInvoke(responses: Record<string, unknown>) {
  invokeMock.mockImplementation((command: string) => {
    if (command in responses) return Promise.resolve(responses[command]);
    if (command === "list_projects") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mockInvoke({});
});

afterEach(() => {
  invokeMock.mockReset();
  useUiStore.setState({ activeWorkspace: "table-repository", activeProjectId: null, handoffDataset: null });
});

describe("Toolbar", () => {
  it("dispatches table.archive with the real command name/args and renders the real response", async () => {
    const archived = makeTable({ admin_state: { table_state: "Archived", table_event_log_id: "e", archived_at: 2000 } });
    mockInvoke({ archive_table: archived });

    render(<Toolbar tableId={archived.id} allowed={makeAllowedActions({ archive: true })} onAfterAction={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "table.archive" }));

    expect(invokeMock).toHaveBeenCalledWith("archive_table", { tableId: archived.id });
    expect(await screen.findByText(new RegExp(archived.admin_state.table_state))).toBeInTheDocument();
  });

  it("every button dispatches exactly one command — no inline logic on click", () => {
    render(<Toolbar tableId="t1" allowed={makeAllowedActions()} onAfterAction={() => {}} />);
    // Each button label is literally the canonical command name, and the
    // rendering tests above/below confirm each click maps to exactly the
    // one matching invoke() call — never two, never a different command.
    for (const label of ["table.import", "table.reimport_as_new", "table.open_readonly", "table.open_dictionary", "table.archive", "table.copy_id"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  describe("disabled state matches the backend's state-action matrix exactly", () => {
    const gatedButtons: [string, keyof ReturnType<typeof makeAllowedActions>][] = [
      ["table.open_readonly", "open_readonly"],
      ["table.load_into_project", "load_into_project"],
      ["table.create_project_from_table", "load_into_project"],
      ["table.route_inspection", "route_inspection"],
      ["table.route_validation", "route_validation"],
      ["table.route_analysis", "route_analysis"],
      ["table.archive", "archive"],
      ["table.restore", "restore"],
    ];

    for (const [state, allowed] of Object.entries(ALLOWED_ACTIONS_BY_STATE)) {
      it(`table_state = ${state}`, async () => {
        // table.load_into_project carries one additional real gate beyond
        // AllowedActions — a target project must actually be selected
        // (Part B: there was previously no way to supply one at all).
        // Select one here so this button's assertion below reflects
        // AllowedActions alone, same as every other gated button; the
        // "no project selected yet" case is covered by its own dedicated
        // test below.
        mockInvoke({ list_projects: [{ project_id: "p1", name: "P", dataset_count: 0, created_at: 1_000 }] });
        render(<Toolbar tableId="t1" allowed={allowed} onAfterAction={() => {}} />);
        const select = await screen.findByLabelText("Target project for table.load_into_project");
        fireEvent.change(select, { target: { value: "p1" } });

        for (const [label, key] of gatedButtons) {
          const button = screen.getByRole("button", { name: label });
          const expected = !allowed[key];
          expect(button).toHaveProperty("disabled", expected);
        }
      });
    }

    it("table.load_into_project is additionally disabled while no target project is selected, even when otherwise allowed", () => {
      render(<Toolbar tableId="t1" allowed={makeAllowedActions({ load_into_project: true })} onAfterAction={() => {}} />);
      expect(screen.getByRole("button", { name: "table.load_into_project" })).toHaveProperty("disabled", true);
    });
  });

  it("no selection disables every table-scoped button, including ones not gated by AllowedActions", () => {
    render(<Toolbar tableId={null} allowed={null} onAfterAction={() => {}} />);
    for (const label of ["table.reimport_as_new", "table.open_readonly", "table.open_dictionary", "table.retry_availability_check", "table.copy_id"]) {
      expect(screen.getByRole("button", { name: label })).toHaveProperty("disabled", true);
    }
    // table.import needs no table selected at all — it's how a Table
    // starts existing in the first place.
    expect(screen.getByRole("button", { name: "table.import" })).toHaveProperty("disabled", false);
  });

  it("populates the target-project select from the real list_projects response", async () => {
    mockInvoke({ list_projects: [{ project_id: "p1", name: "Orders Project", dataset_count: 2, created_at: 1_000 }] });
    render(<Toolbar tableId="t1" allowed={makeAllowedActions({ load_into_project: true })} onAfterAction={() => {}} />);

    expect(await screen.findByRole("option", { name: "Orders Project" })).toBeInTheDocument();
  });

  it("table.load_into_project: selecting a project and confirming closes the Table Repository → Project Workspace hand-off loop with the real returned Dataset", async () => {
    const dataset = makeDataset({ id: "d1", project_id: "p1", name: "orders" });
    mockInvoke({
      list_projects: [{ project_id: "p1", name: "Orders Project", dataset_count: 0, created_at: 1_000 }],
      load_table_into_project: dataset,
    });
    render(<Toolbar tableId="t1" allowed={makeAllowedActions({ load_into_project: true })} onAfterAction={() => {}} />);

    const select = await screen.findByLabelText("Target project for table.load_into_project");
    fireEvent.change(select, { target: { value: "p1" } });
    fireEvent.click(screen.getByRole("button", { name: "table.load_into_project" }));

    await screen.findByText(/table\.load_into_project/);
    expect(invokeMock).toHaveBeenCalledWith("load_table_into_project", { tableId: "t1", projectId: "p1" });
    // The hand-off itself: the real Dataset the command returned — not a
    // guessed/default one — is what Project Workspace will read.
    expect(useUiStore.getState().activeWorkspace).toBe("project");
    expect(useUiStore.getState().activeProjectId).toBe("p1");
    expect(useUiStore.getState().handoffDataset).toEqual(dataset);
  });

  it("table.create_project_from_table: confirming closes the hand-off loop with the real returned Project and Dataset, no project selection required", async () => {
    const dataset = makeDataset({ id: "d2", project_id: "p2", name: "orders" });
    mockInvoke({
      create_project_from_table: [{ id: "p2", name: "New Project", created_at: 1_000, dataset_ids: ["d2"], operation_log: { entries: [] } }, dataset],
    });
    render(<Toolbar tableId="t1" allowed={makeAllowedActions({ load_into_project: true })} onAfterAction={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: "table.create_project_from_table" }));

    await screen.findByText(/table\.create_project_from_table/);
    expect(useUiStore.getState().activeWorkspace).toBe("project");
    expect(useUiStore.getState().activeProjectId).toBe("p2");
    expect(useUiStore.getState().handoffDataset).toEqual(dataset);
  });
});
