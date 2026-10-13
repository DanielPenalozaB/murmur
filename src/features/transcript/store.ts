import { create } from "zustand";
import type { Segment, SessionRecord, TranscriptEvent } from "../../lib/ipc";

type TranscriptState = {
  title: string;
  segments: Segment[];
  /** True between Stop and the backend's `finished` event. */
  finishing: boolean;
  /** The saved session, once the file is written. */
  saved: SessionRecord | null;
  error: string | null;
  /** What the user types in the Notes tab during the meeting. */
  userNotes: string;
  setUserNotes: (notes: string) => void;
  begin: (title: string) => void;
  setFinishing: (finishing: boolean) => void;
  handle: (event: TranscriptEvent) => void;
};

export const useTranscript = create<TranscriptState>((set) => ({
  title: "",
  segments: [],
  finishing: false,
  saved: null,
  error: null,
  userNotes: "",
  setUserNotes: (userNotes) => set({ userNotes }),
  begin: (title) =>
    set({ title, segments: [], finishing: false, saved: null, error: null, userNotes: "" }),
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
        set({ finishing: false, saved: event.session });
        break;
    }
  },
}));
