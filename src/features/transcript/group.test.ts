import { describe, expect, it } from "vitest";
import type { Segment } from "../../lib/ipc";
import { formatTimestamp, groupIntoTurns, speakerLabel } from "./group";

const seg = (
  start: number,
  end: number,
  channel: Segment["channel"],
  text: string,
  speaker: string | null = null,
): Segment => ({ start, end, channel, speaker, text });

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

describe("speakers", () => {
  it("splits turns when the system-audio speaker changes", () => {
    const turns = groupIntoTurns([
      seg(0, 2000, "system", "Hi.", "speaker_1"),
      seg(2500, 4000, "system", "Hello.", "speaker_2"),
      seg(4200, 5000, "system", "Ready?", "speaker_2"),
    ]);
    expect(turns.map((t) => [t.speaker, t.texts.join(" ")])).toEqual([
      ["speaker_1", "Hi."],
      ["speaker_2", "Hello. Ready?"],
    ]);
  });

  it("labels speakers like the session file does", () => {
    const named = [{ id: "speaker_1", name: "Ana", named: true }];
    expect(speakerLabel("mic", null)).toBe("Me");
    expect(speakerLabel("system", null)).toBe("Them");
    expect(speakerLabel("system", "speaker_1", named)).toBe("Ana");
    expect(speakerLabel("system", "speaker_3", named)).toBe("Speaker 3");
  });
});

describe("formatTimestamp", () => {
  it("formats minutes and seconds", () => {
    expect(formatTimestamp(65_400)).toBe("01:05");
  });
});
