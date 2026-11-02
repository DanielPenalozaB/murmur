import type { Channel, Segment, SpeakerInfo } from "../../lib/ipc";

export type Turn = { channel: Channel; speaker: string | null; start: number; texts: string[] };

/** Consecutive segments from the same speaker within `maxGapMs` become one turn. */
export function groupIntoTurns(segments: Segment[], maxGapMs = 4000): Turn[] {
  const sorted = [...segments].sort((a, b) => a.start - b.start);
  const turns: Turn[] = [];
  let lastEnd = 0;
  for (const s of sorted) {
    const last = turns.at(-1);
    if (
      last &&
      last.channel === s.channel &&
      last.speaker === s.speaker &&
      s.start - lastEnd <= maxGapMs
    ) {
      last.texts.push(s.text);
    } else {
      turns.push({ channel: s.channel, speaker: s.speaker, start: s.start, texts: [s.text] });
    }
    lastEnd = Math.max(lastEnd, s.end);
  }
  return turns;
}

/** The turn being said at `ms`: the last one starting at or before it (−1 before the first). */
export function turnAt(turns: Turn[], ms: number) {
  let index = -1;
  turns.forEach((turn, i) => {
    if (turn.start <= ms) index = i;
  });
  return index;
}

/** Same rule as the Rust side: "Me", a speaker's name, "Speaker N", or "Them". */
export function speakerLabel(
  channel: Channel,
  speaker: string | null,
  speakers: SpeakerInfo[] = [],
) {
  if (channel === "mic") return "Me";
  if (!speaker) return "Them";
  const known = speakers.find((s) => s.id === speaker);
  return known?.name ?? `Speaker ${speaker.replace("speaker_", "")}`;
}

export function formatTimestamp(ms: number) {
  const total = Math.floor(ms / 1000);
  const m = String(Math.floor(total / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return `${m}:${s}`;
}
