import { create } from "zustand";

export type View =
  | { kind: "live" }
  | { kind: "folder"; folder: string | null }
  | { kind: "session"; id: string }
  | { kind: "search"; query: string }
  | { kind: "speakers" }
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
export const useUi = create<{ startOpen: boolean; setStartOpen: (open: boolean) => void }>(
  (set) => ({
    startOpen: false,
    setStartOpen: (startOpen) => set({ startOpen }),
  }),
);
