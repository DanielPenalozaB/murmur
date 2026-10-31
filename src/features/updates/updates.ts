import { useQuery } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";

const SIX_HOURS = 6 * 60 * 60 * 1000;

/** The newest release if it's newer than this build, from the GitHub release's latest.json. */
export function useUpdate() {
  return useQuery({
    queryKey: ["update"],
    queryFn: findUpdate,
    // Dev builds have nothing to update to; the browser preview mocks a release.
    enabled: !import.meta.env.DEV || location.search.includes("preview"),
    staleTime: SIX_HOURS,
    refetchInterval: SIX_HOURS,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

/** Offline or no release yet: nothing to offer, and nothing worth an error banner. */
async function findUpdate(): Promise<Update | null> {
  try {
    return await check();
  } catch (e) {
    console.warn("update check failed", e);
    return null;
  }
}

export type InstallPhase =
  | { status: "idle" }
  | { status: "downloading"; fraction: number | null }
  | { status: "restarting" }
  | { status: "failed"; message: string };

/** One install at a time, shared by the sidebar card, its dialog and Settings. */
export const useInstall = create<{
  phase: InstallPhase;
  install: (update: Update) => Promise<void>;
}>((set) => ({
  phase: { status: "idle" },
  install: async (update) => {
    set({ phase: { status: "downloading", fraction: null } });
    try {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          done += event.data.chunkLength;
          set({ phase: { status: "downloading", fraction: total ? done / total : null } });
        }
      });
      set({ phase: { status: "restarting" } });
      await relaunch();
    } catch (e) {
      set({ phase: { status: "failed", message: String(e) } });
    }
  },
}));

/** "Downloading… 42%", or what the button says when nothing is happening. */
export function installLabel(phase: InstallPhase, idle = "Update and restart") {
  switch (phase.status) {
    case "downloading":
      return phase.fraction === null
        ? "Downloading…"
        : `Downloading… ${Math.round(phase.fraction * 100)}%`;
    case "restarting":
      return "Restarting…";
    default:
      return idle;
  }
}
