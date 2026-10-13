import { create } from "zustand";
import { ipc } from "../../lib/ipc";
import { queryClient } from "../../lib/queryClient";

export type Exchange = {
  id: number;
  question: string;
  answer: string;
  status: "streaming" | "done" | "error";
  error?: string;
};

/** "live" for the recording in progress, otherwise a session id. */
export type AskTarget = string;

type AskState = {
  threads: Record<AskTarget, Exchange[]>;
  ask: (target: AskTarget, question: string) => void;
  clear: (target: AskTarget) => void;
};

let nextId = 1;

/** Questions and answers per session, kept for the app's lifetime. */
export const useAsk = create<AskState>((set) => {
  const update = (target: AskTarget, id: number, patch: (e: Exchange) => Partial<Exchange>) =>
    set((s) => ({
      threads: {
        ...s.threads,
        [target]: (s.threads[target] ?? []).map((e) => (e.id === id ? { ...e, ...patch(e) } : e)),
      },
    }));

  return {
    threads: {},
    clear: (target) => set((s) => ({ threads: { ...s.threads, [target]: [] } })),
    ask: (target, question) => {
      const id = nextId++;
      set((s) => ({
        threads: {
          ...s.threads,
          [target]: [
            ...(s.threads[target] ?? []),
            { id, question, answer: "", status: "streaming" },
          ],
        },
      }));
      ipc
        .ask(target === "live" ? null : target, question, (event) => {
          if (event.kind === "text") update(target, id, (e) => ({ answer: e.answer + event.text }));
          if (event.kind === "done") update(target, id, () => ({ status: "done" }));
          if (event.kind === "error")
            update(target, id, () => ({ status: "error", error: event.message }));
        })
        .catch((error) => update(target, id, () => ({ status: "error", error: String(error) })));
    },
  };
});

export type NotesJob = { text: string; status: "writing" | "done" | "error"; error?: string };

type NotesState = {
  jobs: Record<string, NotesJob>;
  generate: (sessionId: string, userNotes: string) => void;
};

/** Notes being written per session. Runs outside any view, so navigating away is fine. */
export const useNotes = create<NotesState>((set) => {
  const update = (id: string, patch: (job: NotesJob) => Partial<NotesJob>) =>
    set((s) => {
      const job = s.jobs[id] ?? { text: "", status: "writing" };
      return { jobs: { ...s.jobs, [id]: { ...job, ...patch(job) } } };
    });

  return {
    jobs: {},
    generate: (sessionId, userNotes) => {
      set((s) => ({ jobs: { ...s.jobs, [sessionId]: { text: "", status: "writing" } } }));
      ipc
        .generateNotes(sessionId, userNotes, (event) => {
          if (event.kind === "text") update(sessionId, (j) => ({ text: j.text + event.text }));
          if (event.kind === "error")
            update(sessionId, () => ({ status: "error", error: event.message }));
          if (event.kind === "done") {
            update(sessionId, () => ({ status: "done", text: event.text }));
            queryClient.invalidateQueries({ queryKey: ["session", sessionId] });
          }
        })
        .catch((error) => update(sessionId, () => ({ status: "error", error: String(error) })));
    },
  };
});
