import { describe, expect, it } from "vitest";
import type { Segment } from "../../lib/ipc";
import { formatTimestamp, groupIntoTurns } from "./group";

const seg = (start: number, end: number, channel: Segment["channel"], text: string): Segment => ({
  start,
  end,
  channel,
  speaker: null,
  text,
});

describe("groupIntoTurns", () => {
  it("merges same-channel segments and splits on speaker change", () => {
    const turns = groupIntoTurns([
      seg(0, 2000, "system", "Hi."),
      seg(2500, 4000, "system", "Ready?"),
      seg(4500, 6000, "mic", "Yes."),
      seg(6500, 7000, "system", "Great."),
    ]);
    expect(turns.map((t) => [t.channel, t.texts.join(" ")])).toEqual([
      ["system", "Hi. Ready?"],
      ["mic", "Yes."],
      ["system", "Great."],
    ]);
  });

  it("sorts out-of-order arrivals and splits on long gaps", () => {
    const turns = groupIntoTurns([seg(20_000, 21_000, "mic", "b"), seg(0, 1000, "mic", "a")]);
    expect(turns.map((t) => t.texts)).toEqual([["a"], ["b"]]);
  });
});

describe("formatTimestamp", () => {
  it("formats minutes and seconds", () => {
    expect(formatTimestamp(65_400)).toBe("01:05");
  });
});
