import type { Project } from "../../utils/ipc";

interface Props {
  project: Project | null;
}

// Secondary panel — read-only Project metadata, same "reflect the
// object, decide nothing" discipline as Table Repository's
// TableSummary/TableInformationInspector. `Project` is a working object
// (INV-003), not evidence — but this panel still renders it read-only;
// mutation happens only through the real commands elsewhere, never a
// field edited in place here.
export function ProjectProperties({ project }: Props) {
  if (!project) {
    return <aside aria-label="Project Properties">No project open.</aside>;
  }

  return (
    <aside aria-label="Project Properties">
      <h2>Project Properties</h2>
      <dl>
        <dt>id</dt>
        <dd>{project.id}</dd>
        <dt>name</dt>
        <dd>{project.name}</dd>
        <dt>created_at</dt>
        <dd>{project.created_at}</dd>
        <dt>dataset_ids</dt>
        <dd>{project.dataset_ids.length}</dd>
        <dt>operation_log entries</dt>
        <dd>{project.operation_log.entries.length}</dd>
      </dl>
    </aside>
  );
}
