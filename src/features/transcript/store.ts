import { create } from "zustand";
import type { Channel, Segment, SessionRecord, SpeakerInfo, TranscriptEvent } from "../../lib/ipc";

/** Text of the utterance in progress, from the fast model. */
export type Draft = { channel: Channel; start: number; text: string };

type TranscriptState = {
  title: string;
  segments: Segment[];
  /** At most one per channel; replaced by final segments when the speaker pauses. */
  drafts: Draft[];
  /** Per channel, how far final transcription has reached (ms). Older drafts are stale. */
  finalUntil: Record<Channel, number>;
  /** Known once the meeting is saved and speakers are identified. */
  speakers: SpeakerInfo[];
  /** A slow step after Stop, e.g. "Identifying speakers…". */
  status: string | null;
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
  setSpeakers: (speakers: SpeakerInfo[]) => void;
  handle: (event: TranscriptEvent) => void;
};

export const useTranscript = create<TranscriptState>((set) => ({
  title: "",
  segments: [],
  drafts: [],
  finalUntil: { mic: 0, system: 0 },
  speakers: [],
  status: null,
  finishing: false,
  saved: null,
  error: null,
  userNotes: "",
  setUserNotes: (userNotes) => set({ userNotes }),
  begin: (title) =>
    set({
      title,
      segments: [],
      drafts: [],
      finalUntil: { mic: 0, system: 0 },
      speakers: [],
      status: null,
      finishing: false,
      saved: null,
      error: null,
      userNotes: "",
    }),
  setFinishing: (finishing) => set({ finishing }),
  setSpeakers: (speakers) => set({ speakers }),
  handle: (event) => {
    switch (event.kind) {
      case "segment":
        set((s) => ({ segments: [...s.segments, event.segment] }));
        break;
      case "draft":
        set((s) =>
          event.start < s.finalUntil[event.channel]
            ? s
            : {
                drafts: [
                  ...s.drafts.filter((d) => d.channel !== event.channel),
                  { channel: event.channel, start: event.start, text: event.text },
                ],
              },
        );
        break;
      case "draft_done":
        set((s) => ({
          finalUntil: { ...s.finalUntil, [event.channel]: event.until },
          drafts: s.drafts.filter((d) => d.channel !== event.channel || d.start >= event.until),
        }));
        break;
      case "error":
        set({ error: event.message });
        break;
      case "status":
        set({ status: event.message });
        break;
      case "finished":
        set((s) => ({
          finishing: false,
          status: null,
          saved: event.session,
          drafts: [],
          // Same lines, now labelled with who said them.
          segments: event.segments.length > 0 ? event.segments : s.segments,
          speakers: event.speakers,
        }));
        break;
    }
  },
}));
