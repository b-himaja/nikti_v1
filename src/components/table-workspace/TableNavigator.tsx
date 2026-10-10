import { useState, type FormEvent } from "react";

import { clearTableSearch, refreshTableRepository, searchTables } from "../../utils/ipc";
import type { Table } from "../../utils/ipc";

type NavigatorView = "All Available Tables" | "Recent Tables" | "Archived Tables" | "Registered/Unavailable Tables" | "Search Results";

const REQUIRED_V1_VIEWS: NavigatorView[] = [
  "All Available Tables",
  "Recent Tables",
  "Archived Tables",
  "Registered/Unavailable Tables",
  "Search Results",
];

interface Props {
  tables: Table[];
  selectedTableId: string | null;
  onSelect: (tableId: string) => void;
  onTablesChanged: (tables: Table[]) => void;
}

// Part B: display-only bucketing over `table_state`, a field the backend
// already computed and returned — never a re-derivation of an allow/deny
// decision (that's `AllowedActions`, fetched separately per Part C and
// used only to gate Toolbar buttons, not to decide what's listed here).
function tablesForView(view: NavigatorView, tables: Table[], searchResults: Table[]): Table[] {
  switch (view) {
    case "All Available Tables":
      return tables.filter((t) => t.admin_state.table_state === "Available");
    case "Recent Tables":
      return [...tables].sort((a, b) => b.created_at - a.created_at).slice(0, 10);
    case "Archived Tables":
      return tables.filter((t) => t.admin_state.table_state === "Archived");
    case "Registered/Unavailable Tables":
      return tables.filter((t) => t.admin_state.table_state === "Registered" || t.admin_state.table_state === "Unavailable");
    case "Search Results":
      return searchResults;
  }
}

export function TableNavigator({ tables, selectedTableId, onSelect, onTablesChanged }: Props) {
  const [view, setView] = useState<NavigatorView>("All Available Tables");
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Table[]>([]);

  async function handleSearch(event: FormEvent) {
    event.preventDefault();
    const results = await searchTables(query);
    setSearchResults(results);
    setView("Search Results");
  }

  async function handleClearSearch() {
    setQuery("");
    setSearchResults([]);
    const results = await clearTableSearch();
    onTablesChanged(results);
    setView("All Available Tables");
  }

  async function handleRefresh() {
    const results = await refreshTableRepository();
    onTablesChanged(results);
  }

  const visible = tablesForView(view, tables, searchResults);

  return (
    <nav aria-label="Table Navigator">
      <h2>Table Navigator</h2>
      <form onSubmit={handleSearch}>
        <label htmlFor="table-search-input">Search tables</label>
        <input id="table-search-input" value={query} onChange={(event) => setQuery(event.target.value)} />
        <button type="submit">table.search</button>
        <button type="button" onClick={handleClearSearch}>
          table.clear_search
        </button>
      </form>
      <button type="button" onClick={handleRefresh}>
        table.refresh_repository
      </button>
      <div role="tablist" aria-label="Table Navigator views">
        {REQUIRED_V1_VIEWS.map((v) => (
          <button key={v} type="button" role="tab" aria-selected={view === v} onClick={() => setView(v)}>
            {v}
          </button>
        ))}
      </div>
      <ul aria-label={`${view} list`}>
        {visible.map((t) => (
          <li key={t.id}>
            <button type="button" aria-current={selectedTableId === t.id} onClick={() => onSelect(t.id)}>
              {t.descriptive_metadata.table_name} ({t.admin_state.table_state})
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
