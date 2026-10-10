import { useState } from "react";

import { ReportViewer } from "../shared/ReportViewer";
import { applyRecipe, loadRecipeIntoProject } from "../../utils/ipc";
import type { Dataset, Project, RecipeContext } from "../../utils/ipc";

type Tab = "Operation Log" | "Recipes" | "Dataset Reports";

interface Props {
  project: Project | null;
  activeDataset: Dataset | null;
  sessionReportIds: string[];
  onDatasetChange: (dataset: Dataset) => void;
}

// Single tabbed bottom panel — Operation Log | Recipes | Dataset
// Reports, same pattern as Table Repository's EventLogReportsPanel.
export function ProjectBottomPanel({ project, activeDataset, sessionReportIds, onDatasetChange }: Props) {
  const [tab, setTab] = useState<Tab>("Operation Log");
  const [loadedRecipes, setLoadedRecipes] = useState<RecipeContext[]>([]);
  const [recipeId, setRecipeId] = useState("");
  const [recipeVersion, setRecipeVersion] = useState("1");
  const [message, setMessage] = useState("");
  const [openReportId, setOpenReportId] = useState<string | null>(null);

  async function handleLoadRecipe() {
    try {
      const context = await loadRecipeIntoProject(recipeId, Number(recipeVersion));
      setLoadedRecipes((prev) => [...prev, context]);
      setMessage(`recipe.load: ${JSON.stringify(context)}`);
    } catch (err) {
      setMessage(`recipe.load: ${String(err)}`);
    }
  }

  async function handleApplyRecipe(context: RecipeContext) {
    if (!activeDataset) return;
    try {
      const dataset = await applyRecipe(activeDataset.id, context.recipe_id);
      onDatasetChange(dataset);
      setMessage(`recipe.apply: ${JSON.stringify(dataset)}`);
    } catch (err) {
      setMessage(`recipe.apply: ${String(err)}`);
    }
  }

  const reportIds = Array.from(new Set([...(activeDataset?.report_refs ?? []), ...sessionReportIds]));

  return (
    <section aria-label="Project Bottom Panel">
      <div role="tablist" aria-label="Project Bottom Panel tabs">
        {(["Operation Log", "Recipes", "Dataset Reports"] as const).map((t) => (
          <button key={t} type="button" role="tab" aria-selected={tab === t} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>

      {tab === "Operation Log" && (
        <section aria-label="Project Operation Log">
          {/* INV-005/INV-021: append-only, strictly read-only — no
             edit/reorder control anywhere on this list, no exceptions. */}
          <ul aria-label="Operation Log Entries">
            {(project?.operation_log.entries ?? []).map((entry) => (
              <li key={entry.id}>
                {entry.recorded_at}: {entry.operation} (dataset {entry.dataset_id})
              </li>
            ))}
          </ul>
          {/* Explicitly excluded from this task: `promote_recipe`'s
             command signature doesn't match the real
             `promote_operation_log_to_recipe` implementation yet (needs
             a project-owning-this-log-entry reverse lookup that doesn't
             exist). Shown disabled, same "not yet available" pattern as
             the Data Dictionary action — never silently functional. */}
          <button type="button" disabled title="Promote to Recipe is not yet available — promote_recipe's command signature does not match the real implementation">
            project_operation_log.promote_to_recipe
          </button>
        </section>
      )}

      {tab === "Recipes" && (
        <section aria-label="Recipes">
          <p>Recipes loaded into this Project this session — there is no command to list a Project's persisted Recipe references (see Project Navigator).</p>
          <label>
            recipe_id
            <input aria-label="recipe_id for recipe.load" value={recipeId} onChange={(e) => setRecipeId(e.target.value)} />
          </label>
          <label>
            version
            <input aria-label="version for recipe.load" value={recipeVersion} onChange={(e) => setRecipeVersion(e.target.value)} />
          </label>
          <button type="button" disabled={!recipeId} onClick={handleLoadRecipe}>
            recipe.load
          </button>
          <ul aria-label="Loaded Recipes">
            {loadedRecipes.map((r) => (
              <li key={`${r.recipe_id}-${r.version}`}>
                {r.recipe_id} v{r.version} ({r.steps.length} step(s)){" "}
                <button type="button" disabled={!activeDataset} onClick={() => handleApplyRecipe(r)}>
                  recipe.apply
                </button>
              </li>
            ))}
          </ul>
          <p role="status">{message}</p>
        </section>
      )}

      {tab === "Dataset Reports" && (
        <section aria-label="Dataset Reports">
          {/* `dataset.report_refs` is real (Dataset carries it directly);
             report ids produced by dataset.run_validation/run_analysis
             this session are added here too since re-fetching the
             Dataset object isn't possible (no get_dataset(id) command).
             Each opens in the same shared ReportViewer Table Repository
             uses — the Report Viewer task built the missing
             open_report command and this component, closing what was
             previously an honest "reference ids only" degradation. */}
          <ul aria-label="Dataset Report References">
            {reportIds.map((id) => (
              <li key={id}>
                <button type="button" onClick={() => setOpenReportId(id)}>
                  {id}
                </button>
              </li>
            ))}
          </ul>
          {reportIds.length === 0 && <p>No reports yet.</p>}
        </section>
      )}

      {openReportId && <ReportViewer reportId={openReportId} onClose={() => setOpenReportId(null)} />}
    </section>
  );
}
