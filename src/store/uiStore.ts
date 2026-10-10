import { create } from "zustand";

import type { Dataset } from "../utils/ipc";

// Frozen technology decision: Zustand holds UI-only state (toasts,
// in-flight progress) — never domain state. Domain state lives behind
// commands (utils/ipc.ts) and is fetched/refetched, not cached here.
//
// `activeWorkspace`/`activeProjectId` are that same kind of state: WHICH
// screen is showing and WHICH already-fetched entity it's looking at is a
// UI concern, not a domain fact.
//
// `handoffDataset` is a deliberate, narrow exception, not a domain cache:
// Table Repository and Project Workspace are two separately-mounted
// top-level trees (switched by App.tsx), so there is no React prop path
// between "the Dataset load_into_project/create_project_from_table just
// returned" and "the screen that navigates to next." This field exists
// ONLY to carry that one already-fetched value across that mount
// boundary — ProjectWorkspace reads it once on mount and clears it
// immediately (see `consumeHandoffDataset`); nothing re-reads it later,
// nothing treats it as authoritative after that point, and every
// subsequent view of Dataset data goes back through a real command, same
// as everywhere else in this app. This is standard SPA "navigation
// state" (the same idea as React Router's `navigate(to, { state })`), not
// a second copy of backend truth.
//
// It also plugs a real gap found while wiring this: there is no
// "get_dataset(id)" or "list Project's Datasets with full detail" command
// anywhere in the command layer — only actions that themselves return a
// fresh Dataset (load_table_as_dataset, duplicate_dataset,
// transform_dataset, apply_recipe, run_pending_plan) ever produce one.
// Without this hand-off, landing in Project Workspace after
// load_into_project would have no way to show the new Dataset at all.
interface Toast {
  id: string;
  message: string;
}

export type ActiveWorkspace = "table-repository" | "project";

interface UiState {
  toasts: Toast[];
  commandInFlight: boolean;
  activeWorkspace: ActiveWorkspace;
  activeProjectId: string | null;
  handoffDataset: Dataset | null;
  pushToast: (message: string) => void;
  dismissToast: (id: string) => void;
  setCommandInFlight: (inFlight: boolean) => void;
  navigateToTableRepository: () => void;
  navigateToProject: (projectId: string, dataset: Dataset) => void;
  consumeHandoffDataset: () => Dataset | null;
}

export const useUiStore = create<UiState>((set, get) => ({
  toasts: [],
  commandInFlight: false,
  activeWorkspace: "table-repository",
  activeProjectId: null,
  handoffDataset: null,
  pushToast: (message) =>
    set((state) => ({
      toasts: [...state.toasts, { id: crypto.randomUUID(), message }],
    })),
  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  setCommandInFlight: (inFlight) => set({ commandInFlight: inFlight }),
  navigateToTableRepository: () => set({ activeWorkspace: "table-repository" }),
  // Part B: the real hand-off — Table Repository's
  // load_into_project/create_project_from_table actions call this with
  // the real Project id and the real, freshly-returned Dataset object,
  // never a guessed or default one.
  navigateToProject: (projectId, dataset) => set({ activeWorkspace: "project", activeProjectId: projectId, handoffDataset: dataset }),
  consumeHandoffDataset: () => {
    const dataset = get().handoffDataset;
    set({ handoffDataset: null });
    return dataset;
  },
}));
