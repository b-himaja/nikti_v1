import { useEffect, useState } from "react";

import { listProjects } from "../../utils/ipc";
import type { Project, ProjectSummary } from "../../utils/ipc";

interface Props {
  activeProjectId: string | null;
  project: Project | null;
  activeDatasetId: string | null;
  onSelectProject: (projectId: string) => void;
}

// Left panel — Project Navigator (per the product spec's own naming,
// not "Explorer"). Lists Projects via list_projects; opening the
// selected one (open_project) is the parent's job — one place fetches
// full Project detail, this component only lists and selects.
//
// "Recipe references" and "Supporting Objects" are part of the product
// spec's own section list for this panel, but no command anywhere
// returns either concept for a Project — `Project` (project.rs) carries
// only `dataset_ids`, nothing recipe- or supporting-object-shaped.
// Rather than invent client-side data to fill those sections, they are
// shown exactly like the Table Repository's "Data Dictionary Service
// not yet available" pattern: present, visibly inert, honest about why
// (INV-011/014 apply to evidence specifically, but the same discipline
// — never fabricate a plausible-looking value for a fact the backend
// doesn't track — applies here too).
//
// A Project's Datasets are listed by id only: `Project` carries
// `dataset_ids: string[]`, and no command resolves an arbitrary id into
// full Dataset detail (only actions that themselves construct/return a
// fresh Dataset — load_table_as_dataset, duplicate_dataset,
// transform_dataset, apply_recipe, run_pending_plan — ever produce one).
// So these rows are informational, not independently clickable — the
// active Dataset shown in DatasetWorkspace is only ever one this session
// actually obtained via one of those real commands (see
// `uiStore.handoffDataset`'s own doc comment for the same gap).
export function ProjectNavigator({ activeProjectId, project, activeDatasetId, onSelectProject }: Props) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);

  useEffect(() => {
    let cancelled = false;
    listProjects().then((result) => {
      if (!cancelled) setProjects(result);
    });
    return () => {
      cancelled = true;
    };
  }, [activeProjectId]);

  return (
    <nav aria-label="Project Navigator">
      <h2>Project Navigator</h2>

      <section aria-label="Projects">
        <h3>Projects</h3>
        <ul>
          {projects.map((p) => (
            <li key={p.project_id}>
              <button type="button" aria-current={p.project_id === activeProjectId} onClick={() => onSelectProject(p.project_id)}>
                {p.name}
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section aria-label="Datasets">
        <h3>Datasets</h3>
        {!project && <p>No project open.</p>}
        {project && project.dataset_ids.length === 0 && <p>No Datasets yet.</p>}
        {project && project.dataset_ids.length > 0 && (
          <ul>
            {project.dataset_ids.map((id) => (
              <li key={id} aria-current={id === activeDatasetId}>
                {id}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-label="Recipe References">
        <h3>Recipe References</h3>
        <p>Not available — no command exists yet to list a Project's Recipe references.</p>
      </section>

      <section aria-label="Supporting Objects">
        <h3>Supporting Objects</h3>
        <p>Not available — no command exists yet for Supporting Objects.</p>
      </section>
    </nav>
  );
}
