import { create } from "zustand";
import type { Segment, TranscriptEvent } from "../../lib/ipc";

type TranscriptState = {
  segments: Segment[];
  /** True between Stop and the backend's `finished` event. */
  finishing: boolean;
  error: string | null;
  reset: () => void;
  setFinishing: (finishing: boolean) => void;
  handle: (event: TranscriptEvent) => void;
};

export const useTranscript = create<TranscriptState>((set) => ({
  segments: [],
  finishing: false,
  error: null,
  reset: () => set({ segments: [], finishing: false, error: null }),
  setFinishing: (finishing) => set({ finishing }),
  handle: (event) => {
    switch (event.kind) {
      case "segment":
        set((s) => ({ segments: [...s.segments, event.segment] }));
        break;
      case "error":
        set({ error: event.message });
        break;
      case "finished":
        set({ finishing: false });
        break;
    }
  },
}));
