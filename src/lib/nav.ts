import { create } from "zustand";

export type View =
  | { kind: "live" }
  | { kind: "folder"; folder: string | null }
  /** `at`: a moment (ms) to show in the transcript. */
  | { kind: "session"; id: string; at?: number }
  | { kind: "search"; query: string }
  | { kind: "speakers" }
  | { kind: "actions" }
  | { kind: "settings" };

/** A breadcrumb; with `to`, clicking it goes there. */
export type Crumb = string | { label: string; to: View };

/** "Vault / Work / 1-1s", each part opening that notebook ("Vault" opens All notes). */
export function folderCrumbs(folder: string | null | undefined): Crumb[] {
  const crumbs: Crumb[] = [{ label: "Vault", to: { kind: "folder", folder: null } }];
  const parts = folder ? folder.split("/") : [];
  parts.forEach((label, i) => {
    crumbs.push({ label, to: { kind: "folder", folder: parts.slice(0, i + 1).join("/") } });
  });
  return crumbs;
}

export const useNav = create<{ view: View; go: (view: View) => void }>((set) => ({
  view: { kind: "live" },
  go: (view) => set({ view }),
}));

/** UI state shared across views. */
export const useUi = create<{
  startOpen: boolean;
  /** Prefills the New session dialog (e.g. from a detected meeting). */
  startTitle: string | null;
  setStartOpen: (open: boolean, title?: string) => void;
}>((set) => ({
  startOpen: false,
  startTitle: null,
  setStartOpen: (startOpen, title) => set({ startOpen, startTitle: title ?? null }),
}));
