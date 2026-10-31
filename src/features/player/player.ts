import { create } from "zustand";

/** The session's recording, both tracks mixed, served by `src-tauri/src/playback.rs`. */
let sourceOf = (sessionId: string) => `murmur-audio://localhost/${encodeURIComponent(sessionId)}`;

/** The preview has no native side, so it plays a stand-in file instead. */
export function setAudioSource(source: (sessionId: string) => string) {
  sourceOf = source;
}

export const RATES = [1, 1.25, 1.5, 2] as const;

export function nextRate(rate: number) {
  const at = RATES.indexOf(rate as (typeof RATES)[number]);
  return RATES[(at + 1) % RATES.length];
}

/** Moves `ms` by `delta`, kept inside the recording. */
export function skip(ms: number, delta: number, durationMs: number) {
  return Math.min(Math.max(0, ms + delta), Math.max(0, durationMs));
}

type PlayerState = {
  sessionId: string | null;
  playing: boolean;
  /** Where playback is, in ms from the start of the session. */
  position: number;
  /** From the file once it's loaded; 0 until then. */
  duration: number;
  rate: number;
  error: string | null;
  load: (sessionId: string) => void;
  unload: () => void;
  /** Plays from `ms`, or from where it is. */
  play: (ms?: number) => void;
  pause: () => void;
  seek: (ms: number) => void;
  setRate: (rate: number) => void;
};

// One element for the whole app: only one recording plays at a time.
let audio: HTMLAudioElement | null = null;

export const usePlayer = create<PlayerState>((set, get) => {
  const element = () => {
    if (audio) return audio;
    audio = new Audio();
    audio.preload = "metadata";
    audio.addEventListener("timeupdate", () => set({ position: (audio?.currentTime ?? 0) * 1000 }));
    audio.addEventListener("durationchange", () => {
      const seconds = audio?.duration ?? 0;
      set({ duration: Number.isFinite(seconds) ? seconds * 1000 : 0 });
    });
    audio.addEventListener("play", () => set({ playing: true, error: null }));
    audio.addEventListener("pause", () => set({ playing: false }));
    audio.addEventListener("ended", () => set({ playing: false }));
    audio.addEventListener("error", () => {
      if (get().sessionId) set({ playing: false, error: "The recording can't be played." });
    });
    return audio;
  };

  return {
    sessionId: null,
    playing: false,
    position: 0,
    duration: 0,
    rate: 1,
    error: null,
    load: (sessionId) => {
      if (get().sessionId === sessionId) return;
      const a = element();
      a.pause();
      a.src = sourceOf(sessionId);
      a.playbackRate = get().rate;
      set({ sessionId, playing: false, position: 0, duration: 0, error: null });
    },
    unload: () => {
      // Clear the session first so the error from emptying `src` isn't shown.
      set({ sessionId: null, playing: false, position: 0, duration: 0, error: null });
      if (!audio) return;
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
    },
    play: (ms) => {
      const a = element();
      if (ms !== undefined) {
        a.currentTime = ms / 1000;
        set({ position: ms });
      }
      a.play().catch((e) => {
        console.warn("playback didn't start", e);
        set({ playing: false, error: "The recording didn't start playing. Try again." });
      });
    },
    pause: () => audio?.pause(),
    seek: (ms) => {
      const a = element();
      a.currentTime = ms / 1000;
      set({ position: ms });
    },
    setRate: (rate) => {
      if (audio) audio.playbackRate = rate;
      set({ rate });
    },
  };
});
