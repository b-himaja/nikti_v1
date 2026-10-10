import { useState } from "react";

import { exportDatasetAsTable } from "../../utils/ipc";
import type { Dataset, Table } from "../../utils/ipc";

interface Props {
  dataset: Dataset;
  onClose: () => void;
}

// `export_dataset_as_table` calls the same real, content-addressed
// registration path as CSV import (`table_workspace::
// register_content_as_table`), so exporting identical content twice
// dedupes to the same underlying Parquet file. This dialog shows
// exactly the real Table the command returns — id and
// table_data_ref.data_fingerprint — so a second export of unchanged
// content visibly returns the SAME table_id/fingerprint, never implying
// a second independent Table was created.
export function ExportDatasetDialog({ dataset, onClose }: Props) {
  const [tableName, setTableName] = useState(dataset.name);
  const [result, setResult] = useState<Table | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleExport() {
    setError(null);
    try {
      const table = await exportDatasetAsTable(dataset.id, tableName);
      setResult(table);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div role="dialog" aria-label="Export Dataset as Table">
      <h2>Export Dataset as Table</h2>
      <label>
        table_name
        <input aria-label="table_name for dataset.export_as_table" value={tableName} onChange={(e) => setTableName(e.target.value)} />
      </label>
      <button type="button" disabled={!tableName} onClick={handleExport}>
        dataset.export_as_table
      </button>
      <button type="button" onClick={onClose}>
        Close
      </button>

      {error && <p role="alert">{error}</p>}

      {result && (
        <dl aria-label="Exported Table">
          <dt>table_id</dt>
          <dd>{result.id}</dd>
          <dt>data_fingerprint</dt>
          <dd>{result.table_data_ref.data_fingerprint}</dd>
        </dl>
      )}
    </div>
  );
}
