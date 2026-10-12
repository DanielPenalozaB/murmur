import { useEffect, useMemo, useRef } from "react";
import { formatTimestamp, groupIntoTurns } from "./group";
import { useTranscript } from "./store";

const LABEL = { mic: "Me", system: "Them" } as const;

export function TranscriptView({ recording }: { recording: boolean }) {
  const { segments, finishing, error } = useTranscript();
  const turns = useMemo(() => groupIntoTurns(segments), [segments]);
  const bottom = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  // Follow new lines unless the user scrolled up to read.
  useEffect(() => {
    if (turns.length > 0 && stickToBottom.current) bottom.current?.scrollIntoView({ block: "end" });
  }, [turns]);

  if (turns.length === 0) {
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
        {finishing && <p className="text-sm text-ink-soft">Finishing transcript…</p>}
        {error && <p className="text-sm text-accent">Transcription error: {error}</p>}
        <div ref={bottom} />
      </div>
    </div>
  );
}
