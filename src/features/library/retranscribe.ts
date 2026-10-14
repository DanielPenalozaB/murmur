import { create } from "zustand";
import { ipc } from "../../lib/ipc";
import { queryClient } from "../../lib/queryClient";

export type RetranscribeJob = {
  status: "running" | "done" | "error";
  /** What's happening now, e.g. "Identifying speakers…". */
  message: string;
  done: number;
  total: number;
  error?: string;
};

type RetranscribeState = {
  jobs: Record<string, RetranscribeJob>;
  run: (sessionId: string, languages: string[]) => void;
};

/** Re-transcriptions per session. Runs outside any view, so navigating away is fine. */
export const useRetranscribe = create<RetranscribeState>((set) => {
  const update = (id: string, patch: Partial<RetranscribeJob>) =>
    set((s) => {
      const job = s.jobs[id] ?? { status: "running", message: "", done: 0, total: 0 };
      return { jobs: { ...s.jobs, [id]: { ...job, ...patch } } };
    });

  return {
    jobs: {},
    run: (sessionId, languages) => {
      update(sessionId, { status: "running", message: "Starting…", done: 0, total: 0 });
      ipc
        .retranscribeSession(sessionId, languages, (event) => {
          if (event.kind === "progress")
            update(sessionId, { done: event.done, total: event.total, message: "Transcribing…" });
          if (event.kind === "status") update(sessionId, { message: event.message });
          if (event.kind === "finished")
            update(sessionId, {
              status: "done",
              message: `Done: ${event.segments} lines, ${event.speakers} speakers.`,
            });
        })
        .catch((error) => update(sessionId, { status: "error", error: String(error) }))
        .finally(() => {
          for (const queryKey of [["session", sessionId], ["sessions"], ["voices"]])
            void queryClient.invalidateQueries({ queryKey });
        });
    },
  };
});

export function progressPercent(job: RetranscribeJob) {
  return job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
}
