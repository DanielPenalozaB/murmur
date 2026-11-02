import { AtSign, Bookmark, NotebookPen, Pencil, Play } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { MomentKind, Segment, SpeakerInfo } from "../../lib/ipc";
import { formatTimestamp, groupIntoTurns, speakerLabel, turnAt } from "./group";

/** A request to scroll to a moment; `nonce` lets the same timestamp be clicked twice. */
export type Seek = { ms: number; nonce: number };

export type Rename = (speakerId: string, name: string) => Promise<void>;

/** A mention, mark or quick note pinned to the turn it happened in. */
export type TurnMark = { at: number; kind: MomentKind; label?: string };

const MARK_LOOK: Record<MomentKind, { icon: typeof AtSign; text: string }> = {
  asked: { icon: AtSign, text: "You were asked" },
  named: { icon: AtSign, text: "You were mentioned" },
  marked: { icon: Bookmark, text: "Marked" },
  note: { icon: NotebookPen, text: "Note" },
};

/** A speaker's name chip. With `onRename`, clicking it edits the name in place. */
export function SpeakerChip({
  label,
  channel,
  speakerId,
  onRename,
  onUnname,
  plain,
}: {
  label: string;
  channel: Segment["channel"];
  speakerId: string | null;
  onRename?: Rename;
  /** Offered while editing a named speaker: back to "Speaker N". */
  onUnname?: () => Promise<void>;
  /** No chip background, for use inside another chip. */
  plain?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tone = plain
    ? "px-0 py-0"
    : channel === "mic"
      ? "bg-accent-soft text-on-accent-soft"
      : "bg-muted text-ink";

  if (draft !== null && speakerId && onRename) {
    const save = async () => {
      if (!draft.trim() || draft.trim() === label) return setDraft(null);
      try {
        await onRename(speakerId, draft.trim());
        setDraft(null);
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    };
    return (
      <span className="inline-flex items-center gap-2">
        <input
          // biome-ignore lint/a11y/noAutofocus: opened by clicking the name
          autoFocus
          className="input w-44 py-0.5 text-[12px]"
          value={draft}
          aria-label={`Rename ${label}`}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
            if (e.key === "Escape") setDraft(null);
          }}
        />
        {onUnname && (
          <button
            type="button"
            className="btn-quiet py-0 text-[12px]"
            title="Wrong person: make them a numbered speaker again"
            // Before the input's blur saves and closes the editor.
            onMouseDown={(e) => e.preventDefault()}
            onClick={async () => {
              try {
                await onUnname();
                setDraft(null);
                setError(null);
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            Remove name
          </button>
        )}
        {error && <span className="text-[12px] text-accent">{error}</span>}
      </span>
    );
  }
  if (speakerId && onRename) {
    return (
      <button
        type="button"
        className={`chip group ${tone} hover:ring-1 hover:ring-accent/40`}
        title="Rename this speaker"
        onClick={() => setDraft(label.startsWith("Speaker ") ? "" : label)}
      >
        {label}
        <Pencil className="size-3 opacity-0 group-hover:opacity-60" />
      </button>
    );
  }
  return <span className={`chip ${tone}`}>{label === "Me" ? "You" : label}</span>;
}

export function TranscriptTurns({
  segments,
  seek,
  speakers,
  onRename,
  onPlay,
  playingAt,
  marks = [],
}: {
  segments: Segment[];
  seek?: Seek | null;
  speakers?: SpeakerInfo[];
  onRename?: Rename;
  /** Plays the recording from a turn's start; without it, timestamps are plain text. */
  onPlay?: (ms: number) => void;
  /** Where the recording is while it plays; that turn is marked. */
  playingAt?: number | null;
  marks?: TurnMark[];
}) {
  const turns = useMemo(() => groupIntoTurns(segments), [segments]);
  const refs = useRef<(HTMLDivElement | null)[]>([]);
  const [highlight, setHighlight] = useState<number | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs per seek request, not per new line
  useEffect(() => {
    if (!seek || turns.length === 0) return;
    // Citations are rounded down to the second.
    const index = Math.max(0, turnAt(turns, seek.ms + 999));
    refs.current[index]?.scrollIntoView({ block: "center", behavior: "smooth" });
    setHighlight(index);
    const timer = setTimeout(() => setHighlight(null), 2000);
    return () => clearTimeout(timer);
  }, [seek]);

  const playing = playingAt == null ? -1 : turnAt(turns, playingAt);
  const marksAt = useMemo(() => {
    const byTurn = new Map<number, TurnMark[]>();
    for (const m of marks) {
      const i = Math.max(0, turnAt(turns, m.at + 999));
      byTurn.set(i, [...(byTurn.get(i) ?? []), m]);
    }
    return byTurn;
  }, [marks, turns]);

  return (
    <div className="flex flex-col gap-7">
      {turns.map((turn, i) => {
        const label = speakerLabel(turn.channel, turn.speaker, speakers);
        const mine = turn.channel === "mic";
        return (
          <div
            key={`${turn.channel}-${turn.speaker}-${turn.start}`}
            ref={(el) => {
              refs.current[i] = el;
            }}
            className={`-mx-4 flex flex-col gap-2 rounded-xl px-4 py-2 transition-colors duration-500 ${
              highlight === i ? "bg-accent-soft/50" : playing === i ? "bg-sidebar" : ""
            } ${marksAt.has(i) ? "border-l-2 border-accent" : ""}`}
          >
            <div className="flex flex-wrap items-center gap-3 text-[13px]">
              <SpeakerChip
                label={label}
                channel={turn.channel}
                speakerId={turn.speaker}
                onRename={onRename}
              />
              {marksAt.get(i)?.map((m) => {
                const look = MARK_LOOK[m.kind];
                return (
                  <span
                    key={`${m.kind}-${m.at}`}
                    className="chip order-last max-w-64 bg-accent-soft text-on-accent-soft"
                    title={m.label}
                  >
                    <look.icon className="size-3 shrink-0" />
                    <span className="truncate">
                      {m.kind === "note" && m.label ? m.label : look.text}
                    </span>
                  </span>
                );
              })}
              {onPlay ? (
                <button
                  type="button"
                  className="group/play flex items-center gap-1 rounded px-1 tabular-nums text-ink-soft hover:bg-muted hover:text-ink"
                  title="Play from here"
                  onClick={() => onPlay(turn.start)}
                >
                  <Play
                    className={`size-3 fill-current ${playing === i ? "text-accent" : "opacity-0 group-hover/play:opacity-100"}`}
                  />
                  {formatTimestamp(turn.start)}
                </button>
              ) : (
                <span className="tabular-nums text-ink-soft">{formatTimestamp(turn.start)}</span>
              )}
            </div>
            <p
              className={
                mine
                  ? "pl-3 font-serif text-[19px] italic leading-[30px] text-ink"
                  : "text-[17px] leading-[29px] text-ink"
              }
            >
              {turn.texts.join(" ")}
            </p>
          </div>
        );
      })}
    </div>
  );
}
