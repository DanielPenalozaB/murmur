import { create } from "zustand";
import type {
  Channel,
  Moment,
  Segment,
  SessionRecord,
  SpeakerInfo,
  TranscriptEvent,
} from "../../lib/ipc";
import { appendMoment } from "../moments/moments";
import type { Seek } from "./TranscriptTurns";

/** Text of the utterance in progress, from the fast model. */
export type Draft = { channel: Channel; start: number; text: string };

/** What a meeting leaves for its notes once it's saved. */
export type Leftover = { userNotes: string; transcribed: boolean };

type TranscriptState = {
  /** Which recording the live view shows; each Start begins a new one. */
  run: number;
  /** Meetings that were still being saved when the next one started, by run. */
  ended: Record<number, Leftover>;
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
  /** Mentions, marks and quick notes so far; each is also a line in `userNotes`. */
  moments: Moment[];
  addMoment: (moment: Moment) => void;
  /** A moment to scroll the live transcript to (from the moment card's Open). */
  focus: Seek | null;
  showMoment: (at: number) => void;
  /** Starts a new recording's live view and returns its run. */
  begin: (title: string) => number;
  setFinishing: (finishing: boolean) => void;
  setSpeakers: (speakers: SpeakerInfo[]) => void;
  /** An event from the recording `run`; once another has started, it no longer shows. */
  handle: (event: TranscriptEvent, run?: number) => void;
  /** The user's notes and whether anything was transcribed, for run's saved session. */
  leftover: (run: number) => Leftover;
};

export const useTranscript = create<TranscriptState>((set, get) => ({
  run: 0,
  ended: {},
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
  moments: [],
  addMoment: (moment) =>
    set((s) => ({
      moments: [...s.moments, moment],
      userNotes: appendMoment(s.userNotes, moment),
    })),
  focus: null,
  showMoment: (at) => set({ focus: { ms: at, nonce: Date.now() } }),
  begin: (title) => {
    const s = get();
    const run = s.run + 1;
    set({
      run,
      // Back-to-back meetings: the last one is still being saved and needs its notes.
      ended: s.finishing
        ? { ...s.ended, [s.run]: { userNotes: s.userNotes, transcribed: s.segments.length > 0 } }
        : s.ended,
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
      moments: [],
      focus: null,
    });
    return run;
  },
  setFinishing: (finishing) => set({ finishing }),
  setSpeakers: (speakers) => set({ speakers }),
  handle: (event, run) => {
    if (run !== undefined && run !== get().run) return;
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
  leftover: (run) => {
    const s = get();
    if (run === s.run) return { userNotes: s.userNotes, transcribed: s.segments.length > 0 };
    const { [run]: left, ...rest } = s.ended;
    set({ ended: rest });
    return left ?? { userNotes: "", transcribed: false };
  },
}));
