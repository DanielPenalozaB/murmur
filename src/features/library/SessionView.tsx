import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { formatDate, formatDuration, ipc, type SessionDetail } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { AskPanel } from "../assistant/AskPanel";
import { Markdown } from "../assistant/Markdown";
import { useNotes } from "../assistant/store";
import { type Seek, TranscriptTurns } from "../transcript/TranscriptTurns";

/** The user's own notes, kept under "## My notes" when notes are generated. */
function userNotesOf(notes: string) {
  const at = notes.indexOf("## My notes");
  return at === -1 ? "" : notes.slice(at + "## My notes".length).trim();
}

function NotesTab({ session, onSeek }: { session: SessionDetail; onSeek: (ms: number) => void }) {
  const queryClient = useQueryClient();
  const job = useNotes((s) => s.jobs[session.id]);
  const generate = useNotes((s) => s.generate);
  const [draft, setDraft] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const writing = job?.status === "writing";
  const hasTranscript = session.segments.length > 0;

  const regenerate = () => {
    if (session.notes.trim() && !window.confirm("Replace the current notes with new ones?")) return;
    generate(session.id, userNotesOf(session.notes));
  };

  if (draft !== null) {
    return (
      <div className="flex flex-col gap-3">
        <textarea
          className="input min-h-96 resize-y font-mono leading-relaxed"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        {saveError && <p className="text-sm text-accent">{saveError}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" className="btn-ghost" onClick={() => setDraft(null)}>
            Cancel
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={async () => {
              try {
                await ipc.saveNotes(session.id, draft);
                await queryClient.invalidateQueries({ queryKey: ["session", session.id] });
                setDraft(null);
                setSaveError(null);
              } catch (e) {
                setSaveError(String(e));
              }
            }}
          >
            Save
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {writing ? (
        <>
          <p className="text-sm text-ink-soft">Writing notes…</p>
          {job.text && <Markdown text={job.text} onSeek={onSeek} />}
        </>
      ) : session.notes.trim() ? (
        <Markdown text={session.notes} onSeek={onSeek} />
      ) : (
        <p className="text-ink-soft">
          {hasTranscript
            ? "No notes yet. Murmur can write a summary, decisions and action items from the transcript."
            : "No speech was transcribed, so there's nothing to write notes from."}
        </p>
      )}
      {job?.status === "error" && <p className="text-sm text-accent">{job.error}</p>}
      {!writing && (
        <div className="flex gap-2">
          {hasTranscript && (
            <button
              type="button"
              className={session.notes.trim() ? "btn-ghost" : "btn-primary"}
              onClick={regenerate}
            >
              {session.notes.trim() ? "Regenerate" : "Write notes"}
            </button>
          )}
          <button type="button" className="btn-ghost" onClick={() => setDraft(session.notes)}>
            Edit
          </button>
        </div>
      )}
    </div>
  );
}

export function SessionView({ id }: { id: string }) {
  const go = useNav((s) => s.go);
  const session = useQuery({ queryKey: ["session", id], queryFn: () => ipc.getSession(id) });
  const [tab, setTab] = useState<"notes" | "transcript">("notes");
  const [seek, setSeek] = useState<Seek | null>(null);
  const s = session.data;

  // Citations jump to the transcript tab, then scroll there once it renders.
  const onSeek = (ms: number) => {
    setTab("transcript");
    setSeek({ ms, nonce: Date.now() });
  };

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_340px]">
      <div className="overflow-y-auto px-8 pb-8 pt-12 select-text">
        <div className="mx-auto max-w-3xl">
          {session.error && <p className="text-accent">{String(session.error)}</p>}
          {s && (
            <>
              <button
                type="button"
                className="text-xs uppercase tracking-wide text-ink-soft hover:text-ink"
                onClick={() => go({ kind: "folder", folder: s.folder })}
              >
                {s.folder ? s.folder.replaceAll("/", " / ") : "Vault root"}
              </button>
              <h2 className="mt-1 font-serif text-3xl">{s.title}</h2>
              <p className="mt-1 text-sm text-ink-soft">
                {formatDate(s.started_at)} · {formatDuration(s.duration_ms)}
              </p>

              <div className="mt-4 flex flex-wrap items-center gap-2 rounded-xl border border-line bg-surface p-3 text-sm">
                <span className="truncate text-ink-soft" title={s.file_path}>
                  {s.file_path.replace(/^\/Users\/[^/]+/, "~")}
                </span>
                <div className="ml-auto flex gap-2">
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={() => ipc.openSessionItem(id, "open_file")}
                  >
                    Open file
                  </button>
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={() => ipc.openSessionItem(id, "reveal_file")}
                  >
                    Reveal in Finder
                  </button>
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={() => ipc.openSessionItem(id, "reveal_audio")}
                  >
                    Audio
                  </button>
                </div>
              </div>

              <div className="mb-5 mt-8 flex gap-1 border-b border-line">
                {(["notes", "transcript"] as const).map((t) => (
                  <button
                    key={t}
                    type="button"
                    onClick={() => setTab(t)}
                    className={`-mb-px border-b-2 px-3 py-2 text-sm ${
                      tab === t
                        ? "border-accent font-medium"
                        : "border-transparent text-ink-soft hover:text-ink"
                    }`}
                  >
                    {t === "notes" ? "Notes" : "Transcript"}
                  </button>
                ))}
              </div>

              {tab === "notes" ? (
                <NotesTab session={s} onSeek={onSeek} />
              ) : s.segments.length === 0 ? (
                <p className="text-ink-soft">No speech was transcribed in this session.</p>
              ) : (
                <TranscriptTurns segments={s.segments} seek={seek} />
              )}
            </>
          )}
        </div>
      </div>
      <aside className="flex min-h-0 flex-col border-l border-line bg-sidebar">
        <h3 className="border-b border-line px-4 pb-2 pt-11 text-sm font-medium">Ask</h3>
        <AskPanel
          target={id}
          onSeek={onSeek}
          disabled={s && s.segments.length === 0 ? "This session has no transcript" : undefined}
        />
      </aside>
    </div>
  );
}
