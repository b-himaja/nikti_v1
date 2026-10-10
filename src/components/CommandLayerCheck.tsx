import { useState } from "react";

import { createProject } from "../utils/ipc";
import { useUiStore } from "../store/uiStore";

// Placeholder only: proves the command layer is reachable end to end
// (React -> utils/ipc.ts -> Tauri IPC -> nikti-app::ipc_commands ->
// nikti-core::commands::create_project). The command itself is a `todo!()`
// stub, so a round trip here is expected to come back as an error until a
// later task implements it for real — that error still demonstrates the
// layering is wired correctly (INV-022/023).
export function CommandLayerCheck() {
  const [result, setResult] = useState<string>("(not yet called)");
  const commandInFlight = useUiStore((s) => s.commandInFlight);
  const setCommandInFlight = useUiStore((s) => s.setCommandInFlight);

  async function handleClick() {
    setCommandInFlight(true);
    try {
      const project = await createProject("Command layer check");
      setResult(JSON.stringify(project));
    } catch (err) {
      setResult(`reached the command layer; command not yet implemented: ${String(err)}`);
    } finally {
      setCommandInFlight(false);
    }
  }

  return (
    <section>
      <h2>Command layer check</h2>
      <button onClick={handleClick} disabled={commandInFlight}>
        {commandInFlight ? "Calling create_project…" : "Call create_project"}
      </button>
      <p>{result}</p>
    </section>
  );
}
