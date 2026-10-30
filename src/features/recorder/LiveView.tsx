import { useQuery } from "@tanstack/react-query";
import { AudioWaveform, Circle, Mic, MicOff, NotebookPen, Pause, Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { AvatarStack, TopBar } from "../../components/ui";
import { ipc, type RecorderStatus } from "../../lib/ipc";
import { useNav, useUi } from "../../lib/nav";
import { AskPanel } from "../assistant/AskPanel";
import { useNotes } from "../assistant/store";
import { formatTimestamp } from "../transcript/group";
import { useTranscript } from "../transcript/store";
import { type Seek, TranscriptTurns } from "../transcript/TranscriptTurns";
import { StartDialog } from "./StartDialog";
import { useRecorder } from "./useRecorder";

export function formatElapsed(ms: number) {
  const total = Math.floor(ms / 1000);
  const h = String(Math.floor(total / 3600)).padStart(2, "0");
  const m = String(Math.floor((total % 3600) / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return `${h}:${m}:${s}`;
}

/** Level bars like the design's waveform, driven by the real meter. */
function Waveform({ level, active }: { level: number; active: boolean }) {
  const shape = [0.35, 0.6, 0.9, 0.55, 1, 0.7, 0.45, 0.8, 0.5, 0.3];
  return (
    <span className="flex h-7 items-center gap-[3px]">
      {shape.map((h, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed decorative bars
          key={i}
          className={`w-[3px] rounded-full transition-[height] duration-100 ${active ? "bg-accent" : "bg-high"}`}
          style={{ height: `${Math.max(12, Math.min(100, h * level * 140))}%` }}
        />
      ))}
    </span>
  );
}

function AudioStreamCard({ status }: { status: RecorderStatus }) {
  const channels = [
    {
      label: status.mic_muted ? "You (mic off)" : "You",
      level: status.mic_muted ? 0 : status.mic_level,
      speaking: status.mic_speaking && !status.mic_muted,
    },
    { label: "Call", level: status.system_level, speaking: status.system_speaking },
  ];
  return (
    <div className="sticky bottom-4 mt-10 flex items-center gap-4 rounded-xl bg-muted/95 px-5 py-4 backdrop-blur">
      <span className="rounded-lg bg-surface p-2 text-ink-soft">
        <AudioWaveform className="size-4" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="font-medium">{status.paused ? "Audio paused" : "Active audio stream"}</p>
        <p className="text-[12px] text-ink-soft">
          Microphone + system audio · 16 kHz · on this Mac
        </p>
      </div>
      {channels.map((c) => (
        <div key={c.label} className="flex items-center gap-2 text-[12px] text-ink-soft">
          <span className={c.speaking ? "font-semibold text-ink" : ""}>{c.label}</span>
          <Waveform level={status.paused ? 0 : c.level} active={c.speaking && !status.paused} />
        </div>
      ))}
      <span className="font-medium tabular-nums">{formatElapsed(status.elapsed_ms)}</span>
    </div>
  );
}

function MyNotesCard() {
  const { userNotes, setUserNotes } = useTranscript();
  return (
    <section className="rounded-xl bg-sidebar p-5">
      <header className="flex items-center justify-between">
        <h3 className="flex items-center gap-2 font-serif text-xl text-ink">
          <NotebookPen className="size-4 text-accent" /> Your notes
        </h3>
        <span className="eyebrow">Expanded when you stop</span>
      </header>
      <textarea
        className="mt-3 min-h-20 w-full resize-none bg-transparent leading-relaxed outline-none placeholder:text-outline field-sizing-content"
        placeholder="Jot down what matters. Murmur expands it into full notes from the transcript."
        value={userNotes}
        onChange={(e) => setUserNotes(e.target.value)}
      />
    </section>
  );
}

function EmptyState({ onStart, busy }: { onStart: () => void; busy: boolean }) {
  return (
    <div className="mx-auto flex max-w-xl flex-1 flex-col items-center justify-center gap-4 pb-24 text-center">
      <span className="rounded-2xl bg-accent-soft p-4 text-on-accent-soft">
        <Mic className="size-6" />
      </span>
      <h1 className="font-serif text-[40px] leading-[48px] tracking-[-0.02em]">
        Ready when you are
      </h1>
      <p className="text-ink-soft">
        Murmur records your microphone and the call's audio, transcribes it on this Mac, and writes
        the notes when you stop.
      </p>
      <button
        type="button"
        className="btn-primary mt-2 px-5 py-2.5"
        onClick={onStart}
        disabled={busy}
      >
        <Circle className="size-3.5 fill-current" /> Start recording
      </button>
    </div>
  );
}

export function LiveView() {
  const { status, start, togglePause, toggleMic, stop, starting, busy, error } = useRecorder();
  const t = useTranscript();
  const notesJob = useNotes((s) => (t.saved ? s.jobs[t.saved.id] : undefined));
  const go = useNav((s) => s.go);
  const { startOpen, startTitle, setStartOpen } = useUi();
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const [seek, setSeek] = useState<Seek | null>(null);
  const bottom = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const recording = status !== null;
  const paused = status?.paused ?? false;
  const micOff = status?.mic_muted ?? false;
  const hasContent = t.segments.length > 0 || t.drafts.length > 0 || t.saved || t.finishing;

  // Follow new lines unless the user scrolled up to read.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when lines or drafts change
  useEffect(() => {
    if (stick.current) bottom.current?.scrollIntoView({ block: "end" });
  }, [t.segments, t.drafts]);

  // Each helper delivers a frame every 100 ms; nothing after 3 s means capture is blocked.
  const silent: string[] = [];
  if (status && status.uptime_ms > 3000) {
    if (!status.mic_receiving) silent.push("microphone");
    if (!status.system_receiving) silent.push("system audio");
  }

  const people = ["Me", ...(t.speakers.length > 0 ? t.speakers.map((s) => s.name) : ["Them"])];
  const title = recording || hasContent ? t.title : "Live";
  const folder = t.saved?.folder ?? settings.data?.last_folder;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={["Vault", ...(folder ? folder.split("/") : []), title || "Live"]} />

      {(recording || hasContent) && (
        <div className="flex items-center gap-4 px-8 pb-4" data-tauri-drag-region>
          {recording && (
            <span className="inline-flex items-center gap-2 rounded-full bg-muted px-3 py-1.5 text-[13px] font-semibold tabular-nums">
              <span
                className={`size-2.5 rounded-full ${paused ? "bg-outline" : "animate-pulse bg-accent"}`}
              />
              {formatElapsed(status.elapsed_ms)}
              {paused && <span className="eyebrow text-[10px]">Paused</span>}
              {micOff && !paused && <span className="eyebrow text-[10px]">Mic off</span>}
            </span>
          )}
          <span className="text-outline">/</span>
          <span className="min-w-0 truncate font-serif text-xl">
            {t.title}
            {recording && <span className="text-ink-soft"> — Live</span>}
          </span>
          <div className="ml-auto flex items-center gap-2">
            {recording ? (
              <>
                <button
                  type="button"
                  className={micOff ? "btn-ghost bg-accent-soft text-on-accent-soft" : "btn-ghost"}
                  onClick={toggleMic}
                  aria-pressed={micOff}
                  title={
                    micOff
                      ? "Murmur isn't listening to your microphone. Click to turn it back on."
                      : "Stop listening to your microphone, e.g. while you're muted in the call. The call's audio keeps recording."
                  }
                >
                  {micOff ? <MicOff className="size-3.5" /> : <Mic className="size-3.5" />}
                  {micOff ? "Mic off" : "Mic on"}
                </button>
                <button type="button" className="btn-ghost" onClick={togglePause}>
                  {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
                  {paused ? "Resume" : "Pause"}
                </button>
                <button type="button" className="btn-primary" onClick={stop} disabled={busy}>
                  <Square className="size-3 fill-current" /> Stop
                </button>
              </>
            ) : (
              <button
                type="button"
                className="btn-primary"
                onClick={() => setStartOpen(true)}
                disabled={busy}
              >
                <Circle className="size-3 fill-current" />
                {starting ? "Loading model…" : "New recording"}
              </button>
            )}
          </div>
        </div>
      )}

      {(error || silent.length > 0 || status?.mic_silent) && (
        <div className="mx-8 mb-3 rounded-lg bg-accent-soft px-4 py-2.5 text-[13px] text-on-accent-soft">
          {error ??
            (silent.length > 0
              ? `No ${silent.join(" or ")} received yet. Check System Settings → Privacy & Security (Microphone / Screen & System Audio Recording) and allow your terminal or Murmur.`
              : "Your microphone is sending pure silence, so nothing you say is captured. It may be muted on the headset itself, or macOS is using another input — check System Settings → Sound → Input.")}
        </div>
      )}

      {!recording && !hasContent ? (
        <EmptyState onStart={() => setStartOpen(true)} busy={busy} />
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[1fr_380px] gap-6 px-8 pb-6">
          <div
            className="min-h-0 overflow-y-auto pr-2 select-text"
            onScroll={(e) => {
              const el = e.currentTarget;
              stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
            }}
          >
            <div className="mx-auto flex max-w-[680px] flex-col gap-8 pt-2">
              <div>
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="eyebrow">Session transcript · Transcribed locally</span>
                  {recording && (
                    <span className="eyebrow flex items-center gap-1.5">
                      <span className="size-1.5 rounded-full bg-local" /> Streaming Whisper turbo
                    </span>
                  )}
                </div>
                <h1 className="mt-3 font-serif text-[40px] leading-[48px] tracking-[-0.02em]">
                  {t.title}
                </h1>
                <div className="mt-4 flex flex-wrap items-center gap-3 text-[14px] text-ink-soft">
                  <AvatarStack names={people} />
                  <span>{people.map((p) => (p === "Me" ? "You" : p)).join(", ")}</span>
                  {t.saved && (
                    <span className="chip ml-auto bg-local-soft text-on-local-soft">
                      Synced to vault
                    </span>
                  )}
                </div>
              </div>

              {recording && <MyNotesCard />}

              <div className="flex items-baseline justify-between">
                <h2 className="font-serif text-2xl">Verbatim transcript</h2>
                <span className="text-[12px] text-ink-soft">
                  {t.speakers.length > 0
                    ? "Labelled by speaker"
                    : "Speakers labelled after the meeting"}
                </span>
              </div>

              {t.segments.length === 0 && t.drafts.length === 0 && recording && (
                <p className="text-ink-soft">Listening… lines appear as people speak.</p>
              )}
              <TranscriptTurns segments={t.segments} seek={seek} speakers={t.speakers} />

              {[...t.drafts]
                .sort((a, b) => a.start - b.start)
                .map((d) => (
                  <div
                    key={d.channel}
                    className="-mt-2 rounded-xl bg-sidebar px-4 py-3"
                    aria-live="polite"
                  >
                    <div className="flex items-center gap-3 text-[13px]">
                      <span
                        className={`chip ${d.channel === "mic" ? "bg-accent-soft text-on-accent-soft" : "bg-high"}`}
                      >
                        {d.channel === "mic" ? "You" : "Them"}
                      </span>
                      <span className="tabular-nums text-ink-soft">{formatTimestamp(d.start)}</span>
                      <span className="ml-auto text-[12px] font-medium text-accent">
                        Live speaking
                      </span>
                    </div>
                    <p className="mt-2 text-[17px] leading-[29px] text-ink-soft">
                      <span className="bg-accent-soft/40">{d.text}</span>
                      <span className="ml-1 inline-block h-4 w-px animate-pulse bg-accent align-middle" />
                    </p>
                  </div>
                ))}

              {(t.finishing || t.status) && (
                <p className="text-[14px] text-ink-soft">{t.status ?? "Finishing transcript…"}</p>
              )}
              {t.error && <p className="text-[14px] text-accent">{t.error}</p>}

              {t.saved && !recording && (
                <div className="card flex flex-wrap items-center gap-3 p-4 text-[14px]">
                  <span className="text-ink-soft">Saved to</span>
                  <span className="truncate font-medium">{t.saved.file_path.split("/").pop()}</span>
                  {notesJob?.status === "writing" && (
                    <span className="text-ink-soft">· writing notes…</span>
                  )}
                  {notesJob?.status === "done" && <span className="text-local">· notes ready</span>}
                  {notesJob?.status === "error" && (
                    <span className="text-accent" title={notesJob.error}>
                      · notes failed
                    </span>
                  )}
                  <button
                    type="button"
                    className="btn-primary ml-auto"
                    onClick={() => t.saved && go({ kind: "session", id: t.saved.id })}
                  >
                    Open notes
                  </button>
                </div>
              )}

              {recording && <AudioStreamCard status={status} />}
              <div ref={bottom} />
            </div>
          </div>

          <div className="flex min-h-0 flex-col">
            <AskPanel
              target="live"
              onSeek={(ms) => setSeek({ ms, nonce: Date.now() })}
              disabled={
                t.segments.length === 0
                  ? "Questions open after the first transcribed line."
                  : undefined
              }
              grounding={
                recording ? "Grounded in the live transcript" : "Grounded in the transcript"
              }
            />
          </div>
        </div>
      )}

      {startOpen && (
        <StartDialog
          initialTitle={startTitle}
          onCancel={() => setStartOpen(false)}
          onStart={(session) => {
            setStartOpen(false);
            start(session);
          }}
        />
      )}
    </div>
  );
}
