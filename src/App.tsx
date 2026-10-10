import { useUiStore } from "./store/uiStore";
import { ProjectWorkspace } from "./workspaces/ProjectWorkspace";
import { TableRepositoryWorkspace } from "./workspaces/TableRepositoryWorkspace";

// Part B of the Project Workspace UI wiring task: Table Repository's
// "Load into Project"/"Create Project from Table" actions now navigate
// here for real via `useUiStore`'s `activeWorkspace` — this is the first
// point where the two previously-separate workspaces are actually
// connected, not just individually reachable.
export function App() {
  const activeWorkspace = useUiStore((s) => s.activeWorkspace);
  return activeWorkspace === "project" ? <ProjectWorkspace /> : <TableRepositoryWorkspace />;
}
