import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useRecorder } from "./useRecorder";

function formatElapsed(ms: number) {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = String(Math.floor((total % 3600) / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${m}:${s}` : `${m}:${s}`;
}

function Meter({ label, level }: { label: string; level: number }) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-12 text-xs text-ink-soft">{label}</span>
      <div className="h-1.5 w-48 overflow-hidden rounded-full bg-muted">
        <div
          className="h-full rounded-full bg-local transition-[width] duration-100"
          style={{ width: `${Math.round(level * 100)}%` }}
        />
      </div>
    </div>
  );
}

export function RecorderPanel() {
  const { status, start, togglePause, stop, lastRecording, busy, error } = useRecorder();
  const recording = status !== null;

  return (
    <div className="flex w-full max-w-md flex-col items-center gap-6">
      <div className="flex items-center gap-3">
        {recording && (
          <span
            className={`size-2.5 rounded-full ${status.paused ? "bg-ink-soft" : "animate-pulse bg-accent"}`}
          />
        )}
        <span className="font-serif text-5xl tabular-nums">
          {formatElapsed(status?.elapsed_ms ?? 0)}
        </span>
      </div>

      <div className="flex flex-col gap-2">
        <Meter label="Me" level={status?.mic_level ?? 0} />
        <Meter label="Them" level={status?.system_level ?? 0} />
      </div>

      <div className="flex gap-2">
        {!recording ? (
          <button type="button" className="btn-primary" onClick={start} disabled={busy}>
            Start recording
          </button>
        ) : (
          <>
            <button type="button" className="btn-ghost" onClick={togglePause}>
              {status.paused ? "Resume" : "Pause"}
            </button>
            <button type="button" className="btn-primary" onClick={stop} disabled={busy}>
              Stop
            </button>
          </>
        )}
      </div>

      {error && <p className="text-center text-sm text-accent">{error}</p>}

      {!recording && lastRecording && (
        <div className="w-full rounded-xl border border-line bg-surface p-4 text-sm">
          <p className="mb-2 font-medium">Saved recording</p>
          {lastRecording.tracks.map((t) => (
            <p key={t.channel} className="text-ink-soft">
              {t.channel === "mic" ? "Me" : "Them"} · {formatElapsed(t.duration_ms)}
            </p>
          ))}
          <button
            type="button"
            className="mt-3 text-accent underline-offset-2 hover:underline"
            onClick={() => revealItemInDir(lastRecording.tracks[0].path)}
          >
            Reveal in Finder
          </button>
        </div>
      )}
    </div>
  );
}
