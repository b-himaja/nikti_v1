import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeTable } from "../../test/fixtures";
import { TableNavigator } from "./TableNavigator";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  invokeMock.mockReset();
});

describe("TableNavigator", () => {
  it("buckets the required V1 views by table_state without calling any command (display-only)", () => {
    const tables = [
      makeTable({ id: "a", descriptive_metadata: { table_name: "available-table", data_dictionary_id: null } }),
      makeTable({ id: "b", descriptive_metadata: { table_name: "archived-table", data_dictionary_id: null }, admin_state: { table_state: "Archived", table_event_log_id: "e", archived_at: 1 } }),
      makeTable({ id: "c", descriptive_metadata: { table_name: "registered-table", data_dictionary_id: null }, admin_state: { table_state: "Registered", table_event_log_id: null, archived_at: null } }),
    ];

    render(<TableNavigator tables={tables} selectedTableId={null} onSelect={() => {}} onTablesChanged={() => {}} />);

    expect(screen.getByText(/available-table/)).toBeInTheDocument();
    expect(screen.queryByText(/archived-table/)).not.toBeInTheDocument();
    expect(screen.queryByText(/registered-table/)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Archived Tables" }));
    expect(screen.getByText(/archived-table/)).toBeInTheDocument();
    expect(screen.queryByText(/available-table/)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Registered/Unavailable Tables" }));
    expect(screen.getByText(/registered-table/)).toBeInTheDocument();

    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("calls table.search with the query and switches to Search Results with the real response", async () => {
    const searchResult = [makeTable({ id: "z", descriptive_metadata: { table_name: "matched", data_dictionary_id: null } })];
    invokeMock.mockResolvedValueOnce(searchResult);

    render(<TableNavigator tables={[]} selectedTableId={null} onSelect={() => {}} onTablesChanged={() => {}} />);

    fireEvent.change(screen.getByLabelText("Search tables"), { target: { value: "matched" } });
    fireEvent.click(screen.getByRole("button", { name: "table.search" }));

    expect(await screen.findByText(/matched/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("search_tables", { query: "matched" });
    expect(screen.getByRole("tab", { name: "Search Results", selected: true })).toBeInTheDocument();
  });

  it("calls table.clear_search and table.refresh_repository as distinct commands", async () => {
    invokeMock.mockResolvedValueOnce([]); // clear_table_search
    const onTablesChanged = vi.fn();

    render(<TableNavigator tables={[]} selectedTableId={null} onSelect={() => {}} onTablesChanged={onTablesChanged} />);

    fireEvent.click(screen.getByRole("button", { name: "table.clear_search" }));
    expect(invokeMock).toHaveBeenCalledWith("clear_table_search", {});
    await waitFor(() => expect(onTablesChanged).toHaveBeenCalledTimes(1));

    invokeMock.mockResolvedValueOnce([]); // refresh_table_repository
    fireEvent.click(screen.getByRole("button", { name: "table.refresh_repository" }));
    expect(invokeMock).toHaveBeenCalledWith("refresh_table_repository", {});
    await waitFor(() => expect(onTablesChanged).toHaveBeenCalledTimes(2));
  });
});
