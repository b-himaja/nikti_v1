import { useEffect, useState } from "react";

import { openTableInformation } from "../../utils/ipc";
import type { Table } from "../../utils/ipc";

interface Props {
  tableId: string;
}

// Compact, read-only, calling table.open_information for the currently
// selected table_id — nothing here decides anything; it only displays
// what the command returns.
export function TableSummary({ tableId }: Props) {
  const [table, setTable] = useState<Table | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTable(null);
    openTableInformation(tableId).then((t) => {
      if (!cancelled) setTable(t);
    });
    return () => {
      cancelled = true;
    };
  }, [tableId]);

  if (!table) {
    return <section aria-label="Selected Table Summary">Loading…</section>;
  }

  return (
    <section aria-label="Selected Table Summary">
      <h2>{table.descriptive_metadata.table_name}</h2>
      <dl>
        <dt>ID</dt>
        <dd>{table.id}</dd>
        <dt>State</dt>
        <dd>{table.admin_state.table_state}</dd>
        <dt>Source type</dt>
        <dd>{table.creation_source.source_type}</dd>
        <dt>Source label</dt>
        <dd>{table.source_provenance.source_label}</dd>
        <dt>Row count</dt>
        <dd>{table.table_data_ref.row_count}</dd>
        <dt>Column count</dt>
        <dd>{table.schema_ref.schema.columns.length}</dd>
        <dt>Created at</dt>
        <dd>{table.created_at}</dd>
        <dt>Created by</dt>
        <dd>{table.created_by}</dd>
      </dl>
    </section>
  );
}
