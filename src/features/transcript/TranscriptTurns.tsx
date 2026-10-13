import { useEffect, useMemo, useRef, useState } from "react";
import type { Segment } from "../../lib/ipc";
import { formatTimestamp, groupIntoTurns } from "./group";

const LABEL = { mic: "Me", system: "Them" } as const;

/** A request to scroll to a moment; `nonce` lets the same timestamp be clicked twice. */
export type Seek = { ms: number; nonce: number };

export function TranscriptTurns({ segments, seek }: { segments: Segment[]; seek?: Seek | null }) {
  const turns = useMemo(() => groupIntoTurns(segments), [segments]);
  const refs = useRef<(HTMLDivElement | null)[]>([]);
  const [highlight, setHighlight] = useState<number | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs per seek request, not per new line
  useEffect(() => {
    if (!seek || turns.length === 0) return;
    // The turn that contains the moment: the last one starting at or before it.
    let index = 0;
    turns.forEach((turn, i) => {
      if (turn.start <= seek.ms + 999) index = i;
    });
    refs.current[index]?.scrollIntoView({ block: "center", behavior: "smooth" });
    setHighlight(index);
    const timer = setTimeout(() => setHighlight(null), 2000);
    return () => clearTimeout(timer);
  }, [seek]);

  return (
    <div className="flex flex-col gap-5">
      {turns.map((turn, i) => (
        <div
          key={`${turn.channel}-${turn.start}`}
          ref={(el) => {
            refs.current[i] = el;
          }}
          className={`-mx-3 flex flex-col gap-1 rounded-lg px-3 py-1 transition-colors duration-500 ${
            highlight === i ? "bg-accent/10" : ""
          }`}
        >
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
