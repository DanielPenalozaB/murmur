import { useMemo } from "react";
import type { Segment } from "../../lib/ipc";
import { formatTimestamp, groupIntoTurns } from "./group";

const LABEL = { mic: "Me", system: "Them" } as const;

export function TranscriptTurns({ segments }: { segments: Segment[] }) {
  const turns = useMemo(() => groupIntoTurns(segments), [segments]);
  return (
    <div className="flex flex-col gap-5">
      {turns.map((turn) => (
        <div key={`${turn.channel}-${turn.start}`} className="flex flex-col gap-1">
          <div className="flex items-center gap-2 text-xs">
            <span
              className={`rounded px-1.5 py-0.5 font-medium ${
                turn.channel === "mic" ? "bg-accent/10 text-accent" : "bg-muted text-ink"
              }`}
            >
              {LABEL[turn.channel]}
            </span>
            <span className="tabular-nums text-ink-soft">{formatTimestamp(turn.start)}</span>
          </div>
          <p className="leading-relaxed">{turn.texts.join(" ")}</p>
        </div>
      ))}
    </div>
  );
}
