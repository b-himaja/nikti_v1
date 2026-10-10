import { useState } from "react";

import { loadPendingPlan, runPendingPlan, savePendingPlan } from "../../utils/ipc";
import type { Dataset, PendingTransformationPlan } from "../../utils/ipc";

interface Props {
  projectId: string | null;
  dataset: Dataset | null;
  onDatasetChange: (dataset: Dataset) => void;
}

// `PendingTransformationPlan` (crates/nikti-core/src/types/
// pending_transformation_plan.rs) is a bare Working Object — it has NO
// `plan_state`/`definition_status` field, unlike the product spec's
// description of this panel. Only the fields the real Rust struct
// actually has are rendered below; nothing is fabricated to match the
// spec's wording.
export function PendingPlanPanel({ projectId, dataset, onDatasetChange }: Props) {
  const [planName, setPlanName] = useState("");
  const [dropColumn, setDropColumn] = useState("");
  const [loadPlanId, setLoadPlanId] = useState("");
  const [plan, setPlan] = useState<PendingTransformationPlan | null>(null);
  const [message, setMessage] = useState("(no command run yet)");

  async function handleSave() {
    if (!projectId || !dataset) return;
    try {
      const saved = await savePendingPlan(projectId, dataset.id, planName, [{ DropColumns: { columns: [dropColumn] } }]);
      setPlan(saved);
      setMessage(`pending_plan.save: ${JSON.stringify(saved)}`);
    } catch (err) {
      setMessage(`pending_plan.save: ${String(err)}`);
    }
  }

  async function handleLoad() {
    try {
      const loaded = await loadPendingPlan(loadPlanId);
      setPlan(loaded);
      setMessage(`pending_plan.load: ${JSON.stringify(loaded)}`);
    } catch (err) {
      setMessage(`pending_plan.load: ${String(err)}`);
    }
  }

  async function handleRun() {
    if (!plan) return;
    try {
      const updated = await runPendingPlan(plan.id);
      onDatasetChange(updated);
      setMessage(`pending_plan.run: ${JSON.stringify(updated)}`);
    } catch (err) {
      setMessage(`pending_plan.run: ${String(err)}`);
    }
  }

  return (
    <section aria-label="Pending Plan Panel">
      <h2>Pending Plan</h2>

      <fieldset>
        <legend>pending_plan.save</legend>
        <label>
          name
          <input aria-label="name for pending_plan.save" value={planName} onChange={(e) => setPlanName(e.target.value)} />
        </label>
        <label>
          column to drop
          <select aria-label="column for pending_plan.save" value={dropColumn} onChange={(e) => setDropColumn(e.target.value)}>
            <option value="">(select a column)</option>
            {(dataset?.schema.columns ?? []).map((c) => (
              <option key={c.name} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <button type="button" disabled={!projectId || !dataset || !planName || !dropColumn} onClick={handleSave}>
          pending_plan.save
        </button>
      </fieldset>

      <fieldset>
        <legend>pending_plan.load</legend>
        <label>
          plan_id
          <input aria-label="plan_id for pending_plan.load" value={loadPlanId} onChange={(e) => setLoadPlanId(e.target.value)} />
        </label>
        <button type="button" disabled={!loadPlanId} onClick={handleLoad}>
          pending_plan.load
        </button>
      </fieldset>

      {plan && (
        <dl aria-label="Loaded Pending Plan">
          <dt>id</dt>
          <dd>{plan.id}</dd>
          <dt>name</dt>
          <dd>{plan.name}</dd>
          <dt>operations</dt>
          <dd>{plan.operations.length}</dd>
          <dt>created_at</dt>
          <dd>{plan.created_at}</dd>
        </dl>
      )}

      <button type="button" disabled={!plan} onClick={handleRun}>
        pending_plan.run
      </button>

      <p role="status">{message}</p>
    </section>
  );
}
