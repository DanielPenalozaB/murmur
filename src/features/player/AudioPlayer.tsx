import { Pause, Play, RotateCcw, RotateCw } from "lucide-react";
import { useEffect } from "react";
import { formatTimestamp } from "../transcript/group";
import { nextRate, skip, usePlayer } from "./player";

const round =
  "flex size-8 shrink-0 items-center justify-center rounded-full text-ink-soft hover:bg-muted hover:text-ink";

/** The session's recording under the notes and transcript: play, skip, scrub, speed. */
export function AudioPlayer({ sessionId, durationMs }: { sessionId: string; durationMs: number }) {
  const { playing, position, duration, rate, error } = usePlayer();
  const { load, unload, play, pause, seek, setRate } = usePlayer.getState();

  useEffect(() => {
    load(sessionId);
    return unload;
  }, [sessionId, load, unload]);

  // The file's length once known; the session's until then.
  const total = duration || durationMs;
  const jump = (delta: number) => seek(skip(position, delta, total));

  return (
    <div className="flex flex-col gap-1 rounded-xl bg-sidebar px-3 py-2 text-[12px] text-ink-soft">
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="flex size-8 shrink-0 items-center justify-center rounded-full bg-accent text-on-accent hover:opacity-90"
          aria-label={playing ? "Pause" : "Play"}
          title={playing ? "Pause" : "Play the recording"}
          onClick={() => (playing ? pause() : play())}
        >
          {playing ? (
            <Pause className="size-3.5 fill-current" />
          ) : (
            <Play className="ml-0.5 size-3.5 fill-current" />
          )}
        </button>
        <button
          type="button"
          className={round}
          title="Back 15 seconds"
          onClick={() => jump(-15_000)}
        >
          <RotateCcw className="size-3.5" />
        </button>
        <button
          type="button"
          className={round}
          title="Forward 15 seconds"
          onClick={() => jump(15_000)}
        >
          <RotateCw className="size-3.5" />
        </button>
        <span className="w-12 shrink-0 text-right tabular-nums">{formatTimestamp(position)}</span>
        <input
          type="range"
          className="h-1 min-w-0 flex-1 cursor-pointer accent-[var(--color-accent)]"
          aria-label="Position in the recording"
          min={0}
          max={Math.max(1, total)}
          step={1000}
          value={Math.min(position, total)}
          onChange={(e) => seek(Number(e.target.value))}
        />
        <span className="w-12 shrink-0 tabular-nums">{formatTimestamp(total)}</span>
        <button
          type="button"
          className="w-11 shrink-0 rounded-md py-1 font-semibold tabular-nums hover:bg-muted hover:text-ink"
          title="Playback speed"
          onClick={() => setRate(nextRate(rate))}
        >
          {rate}×
        </button>
      </div>
      {error && <p className="px-1 text-accent">{error}</p>}
    </div>
  );
}
