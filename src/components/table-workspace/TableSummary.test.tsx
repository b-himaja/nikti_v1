import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeTable } from "../../test/fixtures";
import { TableSummary } from "./TableSummary";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("TableSummary", () => {
  it("calls table.open_information for the selected table and renders the real response shape", async () => {
    const table = makeTable({ descriptive_metadata: { table_name: "customers", data_dictionary_id: null } });
    invokeMock.mockResolvedValueOnce(table);

    render(<TableSummary tableId={table.id} />);

    expect(await screen.findByText("customers")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("open_table_information", { tableId: table.id });
    expect(screen.getByText(table.id)).toBeInTheDocument();
    expect(screen.getByText(table.admin_state.table_state)).toBeInTheDocument();
    expect(screen.getByText(String(table.table_data_ref.row_count))).toBeInTheDocument();
  });
});
