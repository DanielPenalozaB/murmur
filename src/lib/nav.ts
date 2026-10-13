import { create } from "zustand";

export type View =
  | { kind: "live" }
  | { kind: "folder"; folder: string | null }
  | { kind: "session"; id: string }
  | { kind: "search"; query: string }
  | { kind: "speakers" }
  | { kind: "settings" };

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
