import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeTable } from "../../test/fixtures";
import { TableInformationInspector } from "./TableInformationInspector";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("TableInformationInspector", () => {
  it("calls table.open_information and table.open_dictionary, and renders every required nested section read-only", async () => {
    const table = makeTable();
    invokeMock.mockImplementation((command: string) => {
      if (command === "open_table_information") return Promise.resolve(table);
      if (command === "open_table_dictionary") return Promise.resolve("NoDictionaryReferenced");
      throw new Error(`unexpected command ${command}`);
    });

    render(<TableInformationInspector tableId={table.id} />);

    await screen.findByText("Open Table Information");
    expect(invokeMock).toHaveBeenCalledWith("open_table_information", { tableId: table.id });
    expect(invokeMock).toHaveBeenCalledWith("open_table_dictionary", { tableId: table.id });

    // Part B's required sections — every one present, nothing editable.
    for (const section of [
      "Identity",
      "State",
      "Creation Source",
      "Source Provenance",
      "Data Reference",
      "Schema Reference",
      "Data Dictionary Reference",
      "Table Event Log Reference",
      "Report References",
      "Version Information",
      "Administrative State",
    ]) {
      expect(screen.getByLabelText(section)).toBeInTheDocument();
    }

    expect(screen.getByText("No dictionary referenced")).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /edit|delete|reorder/i })).not.toBeInTheDocument();
  });

  it("reflects a referenced-but-not-yet-available dictionary without touching table_state", async () => {
    const table = makeTable({ descriptive_metadata: { table_name: "orders", data_dictionary_id: "66666666-6666-4666-8666-666666666666" } });
    invokeMock.mockImplementation((command: string) => {
      if (command === "open_table_information") return Promise.resolve(table);
      if (command === "open_table_dictionary") return Promise.resolve({ ServiceNotYetAvailable: { data_dictionary_id: "66666666-6666-4666-8666-666666666666" } });
      throw new Error(`unexpected command ${command}`);
    });

    render(<TableInformationInspector tableId={table.id} />);

    expect(await screen.findByText(/Data Dictionary Service not yet available/)).toBeInTheDocument();
    expect(screen.getByText(table.admin_state.table_state)).toBeInTheDocument();
  });
});
