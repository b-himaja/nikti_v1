import { useCallback, useEffect, useState } from "react";

import { EventLogReportsPanel } from "../components/table-workspace/EventLogReportsPanel";
import { TableInformationInspector } from "../components/table-workspace/TableInformationInspector";
import { TableNavigator } from "../components/table-workspace/TableNavigator";
import { TableSummary } from "../components/table-workspace/TableSummary";
import { Toolbar } from "../components/table-workspace/Toolbar";
import { refreshTableRepository, tableAllowedActions } from "../utils/ipc";
import type { AllowedActions, Table } from "../utils/ipc";

// INV-008: this workspace routes to Validation/Analysis/Project/Data
// Dictionary — it never authors or executes any of them itself. Every
// interaction here dispatches one of the 19 canonical `table.*` commands
// (or the read-only `table_allowed_actions` query) via utils/ipc.ts;
// no component decides whether an action is allowed on its own.
export function TableRepositoryWorkspace() {
  const [tables, setTables] = useState<Table[]>([]);
  const [selectedTableId, setSelectedTableId] = useState<string | null>(null);
  const [allowed, setAllowed] = useState<AllowedActions | null>(null);

  const reload = useCallback(async () => {
    const result = await refreshTableRepository();
    setTables(result);
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  useEffect(() => {
    if (!selectedTableId) {
      setAllowed(null);
      return;
    }
    let cancelled = false;
    tableAllowedActions(selectedTableId).then((a) => {
      if (!cancelled) setAllowed(a);
    });
    return () => {
      cancelled = true;
    };
  }, [selectedTableId]);

  async function handleAfterAction() {
    await reload();
    if (selectedTableId) {
      const a = await tableAllowedActions(selectedTableId);
      setAllowed(a);
    }
  }

  return (
    <div>
      <h1>Table Repository Workspace</h1>
      <Toolbar tableId={selectedTableId} allowed={allowed} onAfterAction={handleAfterAction} />
      <div style={{ display: "flex", gap: "1rem" }}>
        <TableNavigator tables={tables} selectedTableId={selectedTableId} onSelect={setSelectedTableId} onTablesChanged={setTables} />
        {selectedTableId && (
          <>
            <TableSummary tableId={selectedTableId} />
            <TableInformationInspector tableId={selectedTableId} />
          </>
        )}
      </div>
      {selectedTableId && <EventLogReportsPanel tableId={selectedTableId} />}
    </div>
  );
}
