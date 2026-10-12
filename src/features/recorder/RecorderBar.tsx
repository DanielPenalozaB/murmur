import { useState } from "react";
import { useTranscript } from "../transcript/store";
import { StartDialog } from "./StartDialog";
import { useRecorder } from "./useRecorder";

function formatElapsed(ms: number) {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = String(Math.floor((total % 3600) / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${m}:${s}` : `${m}:${s}`;
}

function Meter({
  label,
  level,
  speaking,
  dimmed,
}: {
  label: string;
  level: number;
  speaking: boolean;
  dimmed: boolean;
}) {
  return (
    <div className={`flex items-center gap-2 ${dimmed ? "opacity-40" : ""}`}>
      <span className={`w-10 text-xs ${speaking ? "font-medium text-ink" : "text-ink-soft"}`}>
        {label}
      </span>
      <div className="h-1.5 w-28 overflow-hidden rounded-full bg-muted">
        <div
          className="h-full rounded-full bg-local transition-[width] duration-100"
          style={{ width: `${Math.round(level * 100)}%` }}
        />
      </div>
    </div>
  );
}

export function RecorderBar() {
  const { status, start, togglePause, stop, starting, busy, error } = useRecorder();
  const title = useTranscript((s) => s.title);
  const [dialogOpen, setDialogOpen] = useState(false);
  const recording = status !== null;
  const paused = status?.paused ?? false;
  // Each helper delivers a frame every 100 ms; nothing after 3 s means capture is blocked.
  const silent: string[] = [];
  if (status && status.uptime_ms > 3000) {
    if (!status.mic_receiving) silent.push("microphone");
    if (!status.system_receiving) silent.push("system audio");
  }

  return (
    <header className="border-b border-line px-8 pb-4 pt-10" data-tauri-drag-region>
      <div className="flex items-center gap-6">
        <div className="flex items-center gap-3">
          {recording && (
            <span
              className={`size-2.5 rounded-full ${paused ? "bg-ink-soft" : "animate-pulse bg-accent"}`}
            />
          )}
          <span className={`font-serif text-3xl tabular-nums ${paused ? "text-ink-soft" : ""}`}>
            {formatElapsed(status?.elapsed_ms ?? 0)}
          </span>
          {paused && (
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-ink-soft">
              Paused
            </span>
          )}
        </div>

        {recording && title && (
          <span className="max-w-64 truncate font-serif text-lg">{title}</span>
        )}

        <div className="flex flex-col gap-1.5">
          <Meter
            label="Me"
            level={status?.mic_level ?? 0}
            speaking={status?.mic_speaking ?? false}
            dimmed={paused}
          />
          <Meter
            label="Them"
            level={status?.system_level ?? 0}
            speaking={status?.system_speaking ?? false}
            dimmed={paused}
          />
        </div>

        <div className="ml-auto flex items-center gap-2">
          {!recording ? (
            <button
              type="button"
              className="btn-primary"
              onClick={() => setDialogOpen(true)}
              disabled={busy}
            >
              {starting ? "Loading model…" : "Start recording"}
            </button>
          ) : (
            <>
              <button
                type="button"
                className={paused ? "btn-primary" : "btn-ghost"}
                onClick={togglePause}
              >
                {paused ? "Resume" : "Pause"}
              </button>
              <button type="button" className="btn-primary" onClick={stop} disabled={busy}>
                Stop
              </button>
            </>
          )}
        </div>
      </div>
      {error && <p className="mt-3 text-sm text-accent">{error}</p>}
      {dialogOpen && (
        <StartDialog
          onCancel={() => setDialogOpen(false)}
          onStart={(session) => {
            setDialogOpen(false);
            start(session);
          }}
        />
      )}
      {silent.length > 0 && (
        <p className="mt-3 text-sm text-accent">
          No {silent.join(" or ")} received yet. Check System Settings → Privacy &amp; Security
          (Microphone / Screen &amp; System Audio Recording) and allow your terminal or Murmur.
        </p>
      )}
    </header>
  );
}
