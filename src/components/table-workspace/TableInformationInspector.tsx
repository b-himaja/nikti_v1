import { useEffect, useState } from "react";

import { openTableDictionary, openTableInformation } from "../../utils/ipc";
import type { DictionaryNavigation, Table } from "../../utils/ipc";

interface Props {
  tableId: string;
}

function dictionaryLabel(nav: DictionaryNavigation | null): string {
  if (nav === null) return "…";
  if (nav === "NoDictionaryReferenced") return "No dictionary referenced";
  return `Referenced (${nav.ServiceNotYetAvailable.data_dictionary_id}) — Data Dictionary Service not yet available`;
}

// "Open Table Information" (Part A: not "Properties"/"Open Table
// Metadata") — full read-only detail across the canonical nested
// structure. Every field below is read directly off the Table object
// exactly as the backend nests it (Part C) — there is no flattened
// re-derivation here. The interactive `table.open_dictionary` action
// itself lives in the Toolbar (one button per command, per Part B); this
// section only reflects the reference's presence/absence.
export function TableInformationInspector({ tableId }: Props) {
  const [table, setTable] = useState<Table | null>(null);
  const [dictionary, setDictionary] = useState<DictionaryNavigation | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTable(null);
    setDictionary(null);
    Promise.all([openTableInformation(tableId), openTableDictionary(tableId)]).then(([t, d]) => {
      if (cancelled) return;
      setTable(t);
      setDictionary(d);
    });
    return () => {
      cancelled = true;
    };
  }, [tableId]);

  if (!table) {
    return <aside aria-label="Table Information Inspector">Loading…</aside>;
  }

  return (
    <aside aria-label="Table Information Inspector">
      <h2>Open Table Information</h2>

      <section aria-label="Identity">
        <h3>Identity</h3>
        <dl>
          <dt>table_id</dt>
          <dd>{table.id}</dd>
          <dt>table_name</dt>
          <dd>{table.descriptive_metadata.table_name}</dd>
        </dl>
      </section>

      <section aria-label="State">
        <h3>State</h3>
        <dl>
          <dt>table_state</dt>
          <dd>{table.admin_state.table_state}</dd>
        </dl>
      </section>

      <section aria-label="Creation Source">
        <h3>Creation Source</h3>
        <dl>
          <dt>creation_source_type</dt>
          <dd>{table.creation_source.creation_source_type}</dd>
          <dt>source_type</dt>
          <dd>{table.creation_source.source_type}</dd>
        </dl>
      </section>

      <section aria-label="Source Provenance">
        <h3>Source Provenance</h3>
        <dl>
          <dt>source_label</dt>
          <dd>{table.source_provenance.source_label}</dd>
          <dt>source_project_id</dt>
          <dd>{table.source_provenance.source_project_id ?? "—"}</dd>
          <dt>source_dataset_id</dt>
          <dd>{table.source_provenance.source_dataset_id ?? "—"}</dd>
        </dl>
      </section>

      <section aria-label="Data Reference">
        <h3>Data Reference</h3>
        <dl>
          <dt>content_ref</dt>
          <dd>{table.table_data_ref.content_ref}</dd>
          <dt>data_fingerprint</dt>
          <dd>{table.table_data_ref.data_fingerprint}</dd>
          <dt>row_count</dt>
          <dd>{table.table_data_ref.row_count}</dd>
        </dl>
      </section>

      <section aria-label="Schema Reference">
        <h3>Schema Reference</h3>
        <dl>
          <dt>schema_fingerprint</dt>
          <dd>{table.schema_ref.schema_fingerprint}</dd>
          <dt>columns</dt>
          <dd>
            <ul>
              {table.schema_ref.schema.columns.map((c) => (
                <li key={c.name}>
                  {c.name}: {c.data_type}
                </li>
              ))}
            </ul>
          </dd>
        </dl>
      </section>

      <section aria-label="Data Dictionary Reference">
        <h3>Data Dictionary Reference</h3>
        <p>{dictionaryLabel(dictionary)}</p>
      </section>

      <section aria-label="Table Event Log Reference">
        <h3>Table Event Log Reference</h3>
        <dl>
          <dt>table_event_log_id</dt>
          <dd>{table.admin_state.table_event_log_id ?? "—"}</dd>
        </dl>
      </section>

      <section aria-label="Report References">
        <h3>Report References</h3>
        <p>See the Reports tab below.</p>
      </section>

      <section aria-label="Version Information">
        <h3>Version Information</h3>
        <dl>
          <dt>version_family_id</dt>
          <dd>{table.version_info.version_family_id}</dd>
        </dl>
      </section>

      <section aria-label="Administrative State">
        <h3>Administrative State</h3>
        <dl>
          <dt>archived_at</dt>
          <dd>{table.admin_state.archived_at ?? "—"}</dd>
          <dt>created_at</dt>
          <dd>{table.created_at}</dd>
          <dt>created_by</dt>
          <dd>{table.created_by}</dd>
        </dl>
      </section>
    </aside>
  );
}
