import { useCallback, useEffect, useState } from "react";

import { DatasetWorkspace } from "../components/project-workspace/DatasetWorkspace";
import { ExportDatasetDialog } from "../components/project-workspace/ExportDatasetDialog";
import { PendingPlanPanel } from "../components/project-workspace/PendingPlanPanel";
import { ProjectBottomPanel } from "../components/project-workspace/ProjectBottomPanel";
import { ProjectNavigator } from "../components/project-workspace/ProjectNavigator";
import { ProjectProperties } from "../components/project-workspace/ProjectProperties";
import { useUiStore } from "../store/uiStore";
import { openProject } from "../utils/ipc";
import type { Dataset, Project } from "../utils/ipc";

// Part B: the real Table Repository → Project Workspace hand-off. On
// mount, this reads whatever Dataset the store's one-shot
// `handoffDataset` slot is carrying (set by Toolbar's
// table.load_into_project/table.create_project_from_table success
// paths) and consumes it immediately — nothing here re-reads it later,
// matching uiStore's own documented contract for this field.
export function ProjectWorkspace() {
  const activeProjectId = useUiStore((s) => s.activeProjectId);
  const consumeHandoffDataset = useUiStore((s) => s.consumeHandoffDataset);
  const navigateToTableRepository = useUiStore((s) => s.navigateToTableRepository);

  const [project, setProject] = useState<Project | null>(null);
  const [activeDataset, setActiveDataset] = useState<Dataset | null>(null);
  const [sessionReportIds, setSessionReportIds] = useState<string[]>([]);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);

  useEffect(() => {
    const handoff = consumeHandoffDataset();
    if (handoff) setActiveDataset(handoff);
    // Deliberately no dependency on consumeHandoffDataset/activeDataset:
    // this must run exactly once per mount of this screen (the hand-off
    // is one-shot by design), not on every re-render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reloadProject = useCallback(async (projectId: string) => {
    const result = await openProject(projectId);
    setProject(result);
  }, []);

  useEffect(() => {
    if (activeProjectId) reloadProject(activeProjectId);
  }, [activeProjectId, reloadProject]);

  function handleSelectProject(projectId: string) {
    useUiStore.setState({ activeProjectId: projectId });
  }

  function handleDatasetChange(dataset: Dataset) {
    setActiveDataset(dataset);
  }

  function handleReportRegistered(reportId: string) {
    setSessionReportIds((prev) => (prev.includes(reportId) ? prev : [...prev, reportId]));
  }

  return (
    <div>
      <h1>Project Workspace</h1>
      <button type="button" onClick={navigateToTableRepository}>
        Back to Table Repository
      </button>

      <div style={{ display: "flex", gap: "1rem" }}>
        <ProjectNavigator activeProjectId={activeProjectId} project={project} activeDatasetId={activeDataset?.id ?? null} onSelectProject={handleSelectProject} />
        <DatasetWorkspace dataset={activeDataset} onDatasetChange={handleDatasetChange} onOpenExportDialog={() => setExportDialogOpen(true)} onReportRegistered={handleReportRegistered} />
        <div>
          <ProjectProperties project={project} />
          <PendingPlanPanel projectId={activeProjectId} dataset={activeDataset} onDatasetChange={handleDatasetChange} />
        </div>
      </div>

      <ProjectBottomPanel project={project} activeDataset={activeDataset} sessionReportIds={sessionReportIds} onDatasetChange={handleDatasetChange} />

      {exportDialogOpen && activeDataset && <ExportDatasetDialog dataset={activeDataset} onClose={() => setExportDialogOpen(false)} />}
    </div>
  );
}
