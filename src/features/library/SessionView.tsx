import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AudioLines,
  Check,
  ExternalLink,
  FileText,
  FolderOpen,
  Lock,
  PencilLine,
  RefreshCw,
  RotateCcw,
  UsersRound,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Avatar, TopBar } from "../../components/ui";
import { formatDate, formatDuration, ipc, type SessionDetail } from "../../lib/ipc";
import { folderCrumbs, useNav } from "../../lib/nav";
import { AskPanel } from "../assistant/AskPanel";
import { Markdown } from "../assistant/Markdown";
import { NotesDocument } from "../assistant/NotesDocument";
import { actionItems, toggleAction } from "../assistant/notes";
import { useNotes } from "../assistant/store";
import { AudioPlayer } from "../player/AudioPlayer";
import { usePlayer } from "../player/player";
import { refreshSpeakers } from "../speakers/review";
import {
  type Rename,
  type Seek,
  SpeakerChip,
  TranscriptTurns,
} from "../transcript/TranscriptTurns";
import { SessionMenu } from "./manage";
import { RetranscribeDialog } from "./RetranscribeDialog";
import { progressPercent, useRetranscribe } from "./retranscribe";

/** The user's own notes, kept under "## My notes" when notes are generated. */
export function userNotesOf(notes: string) {
  const at = notes.indexOf("## My notes");
  return at === -1 ? "" : notes.slice(at + "## My notes".length).trim();
}

type Decline = (speakerId: string, name: string) => Promise<void>;

function Attendees({
  session,
  onRename,
  onUnname,
  onDecline,
}: {
  session: SessionDetail;
  onRename: Rename;
  onUnname: (speakerId: string) => Promise<void>;
  onDecline: Decline;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl bg-sidebar px-4 py-3">
      <span className="eyebrow mr-1 flex items-center gap-1.5">
        <UsersRound className="size-3.5" /> Attendees
      </span>
      <span className="chip gap-1.5 bg-surface py-1">
        <Avatar name="Me" size="xs" /> Me <span className="text-ink-soft">(you)</span>
      </span>
      {session.speakers.map((s) => (
        <span key={s.id} className="chip gap-1.5 bg-surface py-1">
          <Avatar name={s.name} size="xs" />
          <SpeakerChip
            label={s.name}
            channel="system"
            speakerId={s.id}
            onRename={onRename}
            onUnname={s.named ? () => onUnname(s.id) : undefined}
            plain
          />
          {s.suggestion && (
            <Suggestion
              name={s.suggestion}
              onAccept={() => onRename(s.id, s.suggestion as string)}
              onDecline={() => onDecline(s.id, s.suggestion as string)}
            />
          )}
        </span>
      ))}
      {session.speakers.length === 0 && session.segments.some((s) => s.channel === "system") && (
        <span className="chip gap-1.5 bg-surface py-1">
          <Avatar name="Them" size="xs" /> Them
        </span>
      )}
      {session.speakers.some((s) => !s.named) && (
        <span className="ml-auto text-[12px] text-ink-soft">
          Click a speaker to name them; Murmur remembers their voice.
        </span>
      )}
    </div>
  );
}

/** "Ana?" next to an unnamed speaker whose voice Murmur recognizes. */
function Suggestion({
  name,
  onAccept,
  onDecline,
}: {
  name: string;
  onAccept: () => Promise<void>;
  onDecline: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const run = (action: () => Promise<void>) => async () => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };
  return (
    <span className="flex items-center gap-1 border-l border-line pl-1.5 text-ink-soft">
      <span title={`Sounds like ${name}`}>{name}?</span>
      <button
        type="button"
        className="rounded-full p-0.5 text-accent hover:bg-accent-soft disabled:opacity-50"
        title={`Yes, it's ${name}`}
        aria-label={`Yes, it's ${name}`}
        disabled={busy}
        onClick={run(onAccept)}
      >
        <Check className="size-3.5" />
      </button>
      <button
        type="button"
        className="rounded-full p-0.5 hover:bg-muted disabled:opacity-50"
        title={`Not ${name}`}
        aria-label={`Not ${name}`}
        disabled={busy}
        onClick={run(onDecline)}
      >
        <X className="size-3.5" />
      </button>
    </span>
  );
}

function NotesTab({ session, onSeek }: { session: SessionDetail; onSeek: (ms: number) => void }) {
  const queryClient = useQueryClient();
  const job = useNotes((s) => s.jobs[session.id]);
  const generate = useNotes((s) => s.generate);
  const [draft, setDraft] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  // Browser dialogs (window.confirm) don't show in the app's web view, so confirm inline.
  const [confirmReplace, setConfirmReplace] = useState(false);
  const writing = job?.status === "writing";
  const hasTranscript = session.segments.length > 0;

  const save = async (notes: string) => {
    try {
      await ipc.saveNotes(session.id, notes);
      await queryClient.invalidateQueries({ queryKey: ["session", session.id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await queryClient.invalidateQueries({ queryKey: ["tasks"] });
      setDraft(null);
      setSaveError(null);
    } catch (e) {
      setSaveError(String(e));
    }
  };
  const regenerate = () => {
    setConfirmReplace(false);
    generate(session.id, userNotesOf(session.notes));
  };

  if (draft !== null) {
    return (
      <div className="flex flex-col gap-3">
        <textarea
          className="input min-h-[60vh] resize-y bg-surface font-mono text-[13px] leading-relaxed"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        {saveError && <p className="text-sm text-accent">{saveError}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" className="btn-ghost" onClick={() => setDraft(null)}>
            Cancel
          </button>
          <button type="button" className="btn-primary" onClick={() => save(draft)}>
            Save
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 pb-20">
      {writing ? (
        <>
          <p className="flex items-center gap-2 text-[14px] text-ink-soft">
            <RefreshCw className="size-4 animate-spin" /> Writing notes…
          </p>
          {job.text && <Markdown text={job.text} onSeek={onSeek} />}
        </>
      ) : session.notes.trim() ? (
        <NotesDocument
          notes={session.notes}
          onSeek={onSeek}
          onToggleAction={(line) => save(toggleAction(session.notes, line))}
        />
      ) : (
        <div className="card flex flex-col items-start gap-3 p-6">
          <p className="font-serif text-xl">No notes yet</p>
          <p className="text-ink-soft">
            {hasTranscript
              ? "Murmur can write a summary, decisions and action items from the transcript."
              : "No speech was transcribed, so there's nothing to write notes from."}
          </p>
          {hasTranscript && (
            <button type="button" className="btn-primary" onClick={regenerate}>
              Write notes
            </button>
          )}
        </div>
      )}
      {job?.status === "error" && <p className="text-sm text-accent">{job.error}</p>}
      {saveError && <p className="text-sm text-accent">{saveError}</p>}

      {!writing && session.notes.trim() && (
        <div className="sticky bottom-4 mx-auto flex items-center gap-1 rounded-full bg-ink px-2 py-1.5 text-canvas shadow-xl">
          <button
            type="button"
            className="flex items-center gap-1.5 rounded-full px-3 py-1 text-[13px] hover:bg-canvas/15"
            onClick={() => setDraft(session.notes)}
          >
            <PencilLine className="size-3.5" /> Edit
          </button>
          {hasTranscript &&
            (confirmReplace ? (
              <>
                <span className="px-2 text-[13px]">Replace the notes?</span>
                <button
                  type="button"
                  className="rounded-full bg-canvas/20 px-3 py-1 text-[13px] hover:bg-canvas/30"
                  onClick={regenerate}
                >
                  Replace
                </button>
                <button
                  type="button"
                  className="rounded-full px-3 py-1 text-[13px] hover:bg-canvas/15"
                  onClick={() => setConfirmReplace(false)}
                >
                  Cancel
                </button>
              </>
            ) : (
              <button
                type="button"
                className="flex items-center gap-1.5 rounded-full px-3 py-1 text-[13px] hover:bg-canvas/15"
                onClick={() => setConfirmReplace(true)}
              >
                <RefreshCw className="size-3.5" /> Regenerate
              </button>
            ))}
          <button
            type="button"
            className="flex items-center gap-1.5 rounded-full bg-accent px-3 py-1 text-[13px] text-on-accent"
            onClick={() => ipc.openSessionItem(session.id, "open_file")}
          >
            <ExternalLink className="size-3.5" /> Open file
          </button>
        </div>
      )}
    </div>
  );
}

export function SessionView({ id, at }: { id: string; at?: number }) {
  const go = useNav((s) => s.go);
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: ["session", id], queryFn: () => ipc.getSession(id) });
  const [tab, setTab] = useState<"notes" | "transcript">("notes");
  const [seek, setSeek] = useState<Seek | null>(null);
  const [retranscribing, setRetranscribing] = useState(false);
  const job = useRetranscribe((st) => st.jobs[id]);
  const running = job?.status === "running";
  const s = session.data;
  const playing = usePlayer((p) => p.playing);
  const position = usePlayer((p) => p.position);
  const canPlay = !!s && s.audio_bytes > 0;

  // Citations jump to the transcript tab, then scroll there once it renders.
  // The player follows, so the moment is ready to hear.
  const onSeek = (ms: number) => {
    setTab("transcript");
    setSeek({ ms, nonce: Date.now() });
    if (canPlay) usePlayer.getState().seek(ms);
  };
  const rename: Rename = async (speakerId, name) => {
    await ipc.renameSpeaker(id, speakerId, name);
    await refreshSpeakers(queryClient, id);
  };
  const unname = async (speakerId: string) => {
    await ipc.unnameSpeaker(id, speakerId);
    await refreshSpeakers(queryClient, id);
  };
  const decline: Decline = async (speakerId, name) => {
    await ipc.declineSuggestion(id, speakerId, name);
    await queryClient.invalidateQueries({ queryKey: ["session", id] });
  };
  const actions = s ? actionItems(s.notes) : [];
  const loaded = !!s;

  // Opened at a moment (e.g. from an action item): show it once the session is in.
  // biome-ignore lint/correctness/useExhaustiveDependencies: once per opening, not per render
  useEffect(() => {
    if (loaded && at !== undefined) onSeek(at);
  }, [loaded, at]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={[...folderCrumbs(s?.folder), s?.title ?? ""]}>
        {s && (
          <>
            <button
              type="button"
              className="btn-quiet"
              title={
                s.audio_bytes > 0
                  ? "Transcribe this session again"
                  : "The recordings were deleted, so it can't be transcribed again"
              }
              disabled={running || s.audio_bytes === 0}
              onClick={() => setRetranscribing(true)}
            >
              <RotateCcw className={`size-4 ${running ? "animate-spin" : ""}`} />
            </button>
            <button
              type="button"
              className="btn-quiet"
              title="Open file"
              onClick={() => ipc.openSessionItem(id, "open_file")}
            >
              <FileText className="size-4" />
            </button>
            <button
              type="button"
              className="btn-quiet"
              title="Reveal in Finder"
              onClick={() => ipc.openSessionItem(id, "reveal_file")}
            >
              <FolderOpen className="size-4" />
            </button>
            <button
              type="button"
              className="btn-quiet"
              title="Show audio"
              disabled={s.audio_bytes === 0}
              onClick={() => ipc.openSessionItem(id, "reveal_audio")}
            >
              <AudioLines className="size-4" />
            </button>
            <SessionMenu session={s} busy={running} />
          </>
        )}
      </TopBar>

      <div className="grid min-h-0 flex-1 grid-cols-[1fr_380px] gap-6 px-8 pb-6">
        <div className="flex min-h-0 flex-col gap-3">
          <div className="min-h-0 flex-1 overflow-y-auto pr-2 select-text">
            <div className="mx-auto flex max-w-[720px] flex-col gap-6 pt-2">
              {session.error && <p className="text-accent">{String(session.error)}</p>}
              {s && (
                <>
                  <div className="flex flex-wrap items-center gap-2 text-[12px] font-medium">
                    <span className="chip gap-1.5 bg-muted py-1">
                      <Lock className="size-3" /> Local file · .
                      {s.format === "markdown" ? "md" : "txt"}
                    </span>
                    <span className="text-outline">/</span>
                    <span className="chip gap-1.5 bg-muted py-1">
                      <AudioLines className="size-3" /> {formatDuration(s.duration_ms)} captured
                    </span>
                  </div>
                  <div>
                    <div className="flex flex-wrap items-center gap-2 text-[13px] text-ink-soft">
                      <button
                        type="button"
                        className="chip bg-local-soft uppercase tracking-wide text-on-local-soft"
                        onClick={() => go({ kind: "folder", folder: s.folder })}
                      >
                        {s.folder || "Vault root"}
                      </button>
                      <span>•</span>
                      <span>{formatDate(s.started_at)}</span>
                      <span>•</span>
                      <span>Transcribed locally</span>
                    </div>
                    <h1 className="mt-3 font-serif text-[40px] leading-[48px] tracking-[-0.02em]">
                      {s.title}
                    </h1>
                  </div>

                  {job && job.status !== "done" && (
                    <div
                      className={`rounded-xl px-4 py-3 text-[13px] ${job.status === "error" ? "bg-accent-soft text-on-accent-soft" : "bg-sidebar"}`}
                    >
                      {job.status === "error" ? (
                        <p>Re-transcription failed: {job.error}</p>
                      ) : (
                        <>
                          <p className="flex items-center justify-between gap-3">
                            <span className="flex items-center gap-2">
                              <RotateCcw className="size-3.5 animate-spin" /> Re-transcribing ·{" "}
                              {job.message}
                            </span>
                            {job.total > 0 && (
                              <span className="tabular-nums text-ink-soft">
                                {progressPercent(job)}%
                              </span>
                            )}
                          </p>
                          <div className="mt-2 h-1 overflow-hidden rounded-full bg-high">
                            <div
                              className="h-full bg-accent transition-[width]"
                              style={{ width: `${progressPercent(job)}%` }}
                            />
                          </div>
                        </>
                      )}
                    </div>
                  )}
                  {job?.status === "done" && (
                    <p className="text-[13px] text-local">
                      Re-transcribed. {job.message} Regenerate the notes to match the new
                      transcript.
                    </p>
                  )}

                  <Attendees session={s} onRename={rename} onUnname={unname} onDecline={decline} />

                  <div className="flex justify-center gap-2">
                    {(["notes", "transcript"] as const).map((t) => (
                      <button
                        key={t}
                        type="button"
                        onClick={() => setTab(t)}
                        className={`rounded-lg px-3 py-1.5 text-[13px] font-medium ${
                          tab === t ? "bg-ink text-canvas" : "bg-muted text-ink hover:bg-high"
                        }`}
                      >
                        {t === "notes" ? "Notes" : "Transcript"}
                      </button>
                    ))}
                    {actions.length > 0 && (
                      <button
                        type="button"
                        className="rounded-lg bg-muted px-3 py-1.5 text-[13px] font-medium hover:bg-high"
                        onClick={() => {
                          setTab("notes");
                          setTimeout(() =>
                            document
                              .getElementById("action-items")
                              ?.scrollIntoView({ behavior: "smooth" }),
                          );
                        }}
                      >
                        Action items ({actions.length})
                      </button>
                    )}
                  </div>

                  {tab === "notes" ? (
                    <NotesTab session={s} onSeek={onSeek} />
                  ) : s.segments.length === 0 ? (
                    <p className="text-ink-soft">No speech was transcribed in this session.</p>
                  ) : (
                    <div className="pb-10">
                      <TranscriptTurns
                        segments={s.segments}
                        seek={seek}
                        speakers={s.speakers}
                        onRename={rename}
                        onPlay={canPlay ? (ms) => usePlayer.getState().play(ms) : undefined}
                        playingAt={canPlay && (playing || position > 0) ? position : null}
                      />
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
          {s && canPlay && (
            <div className="mx-auto w-full max-w-[720px] pr-2">
              <AudioPlayer sessionId={id} durationMs={s.duration_ms} />
            </div>
          )}
        </div>
        <div className="flex min-h-0 flex-col">
          <AskPanel
            target={id}
            onSeek={onSeek}
            disabled={s && s.segments.length === 0 ? "This session has no transcript." : undefined}
          />
        </div>
      </div>
      {retranscribing && s && (
        <RetranscribeDialog session={s} onClose={() => setRetranscribing(false)} />
      )}
    </div>
  );
}
