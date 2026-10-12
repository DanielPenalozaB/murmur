import type { Channel, Segment } from "../../lib/ipc";

export type Turn = { channel: Channel; start: number; texts: string[] };

/** Consecutive segments from the same channel within `maxGapMs` become one turn. */
export function groupIntoTurns(segments: Segment[], maxGapMs = 4000): Turn[] {
  const sorted = [...segments].sort((a, b) => a.start - b.start);
  const turns: Turn[] = [];
  let lastEnd = 0;
  for (const s of sorted) {
    const last = turns.at(-1);
    if (last && last.channel === s.channel && s.start - lastEnd <= maxGapMs) {
      last.texts.push(s.text);
    } else {
      turns.push({ channel: s.channel, start: s.start, texts: [s.text] });
    }
    lastEnd = Math.max(lastEnd, s.end);
  }
  return turns;
}

export function formatTimestamp(ms: number) {
  const total = Math.floor(ms / 1000);
  const m = String(Math.floor(total / 60)).padStart(2, "0");
  const s = String(total % 60).padStart(2, "0");
  return `${m}:${s}`;
}
