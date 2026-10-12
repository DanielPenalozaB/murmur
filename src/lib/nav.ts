import { create } from "zustand";

export type View =
  | { kind: "live" }
  | { kind: "folder"; folder: string | null }
  | { kind: "session"; id: string }
  | { kind: "search"; query: string };

export const useNav = create<{ view: View; go: (view: View) => void }>((set) => ({
  view: { kind: "live" },
  go: (view) => set({ view }),
}));
