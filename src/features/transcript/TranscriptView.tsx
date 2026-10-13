import { useEffect, useRef } from "react";
import { ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { useNotes } from "../assistant/store";
import { useTranscript } from "./store";
import { type Seek, TranscriptTurns } from "./TranscriptTurns";

export function TranscriptView({ recording, seek }: { recording: boolean; seek: Seek | null }) {
  const { segments, finishing, saved, error } = useTranscript();
  const notes = useNotes((s) => (saved ? s.jobs[saved.id] : undefined));
  const go = useNav((s) => s.go);
  const bottom = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  // Follow new lines unless the user scrolled up to read.
  useEffect(() => {
    if (segments.length > 0 && stickToBottom.current)
      bottom.current?.scrollIntoView({ block: "end" });
  }, [segments]);

  if (segments.length === 0 && !saved && !finishing) {
    return (
      <div className="flex flex-1 items-center justify-center text-ink-soft">
        {recording
          ? "Listening… lines appear after each pause in speech."
          : "Start a recording to see the live transcript."}
      </div>
    );
  }

  return (
    <div
      className="flex-1 overflow-y-auto px-8 py-6 select-text"
      onScroll={(e) => {
        const el = e.currentTarget;
        stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
      }}
    >
      <div className="mx-auto flex max-w-3xl flex-col gap-5">
        <TranscriptTurns segments={segments} seek={seek} />
        {finishing && <p className="text-sm text-ink-soft">Finishing transcript…</p>}
        {error && <p className="text-sm text-accent">{error}</p>}
        {saved && !recording && (
          <div className="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-surface p-4 text-sm">
            <span className="text-ink-soft">Saved to</span>
            <span className="truncate font-medium">{saved.file_path.split("/").pop()}</span>
            {notes?.status === "writing" && <span className="text-ink-soft">· writing notes…</span>}
            {notes?.status === "done" && <span className="text-local">· notes ready</span>}
            {notes?.status === "error" && (
              <span className="text-accent" title={notes.error}>
                · notes failed
              </span>
            )}
            <div className="ml-auto flex gap-2">
              <button
                type="button"
                className="btn-ghost"
                onClick={() => ipc.openSessionItem(saved.id, "open_file")}
              >
                Open file
              </button>
              <button
                type="button"
                className="btn-ghost"
                onClick={() => ipc.openSessionItem(saved.id, "reveal_file")}
              >
                Reveal in Finder
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={() => go({ kind: "session", id: saved.id })}
              >
                View in library
              </button>
            </div>
          </div>
        )}
        <div ref={bottom} />
      </div>
    </div>
  );
}
