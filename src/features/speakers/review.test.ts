import { expect, describe as group, it } from "vitest";
import type { ReviewSpeaker } from "../../lib/ipc";
import { describe, percent } from "./review";

const speaker = (session_id: string, label: string): ReviewSpeaker => ({
  session_id,
  title: `Standup ${session_id}`,
  started_at: "2026-10-29T10:33:46-05:00",
  speaker_id: label.replace("Speaker ", "speaker_"),
  label,
});

group("speaker review", () => {
  it("rounds similarity to a percentage", () => {
    expect(percent(0.314)).toBe("31%");
    expect(percent(-0.05)).toBe("0%");
  });

  it("explains two voices with one name in the same meeting", () => {
    const { title, detail } = describe({
      kind: "pair",
      key: "k",
      name: "Ruben",
      score: 0.31,
      speakers: [speaker("a", "Speaker 9"), speaker("a", "Speaker 10")],
    });
    expect(title).toBe("Two different voices named Ruben?");
    expect(detail).toContain("In “Standup a”, Speaker 9 and Speaker 10 sound 31% alike.");
    expect(detail).toContain("over 50% alike");
  });

  it("names both meetings when the pair spans two", () => {
    const { detail } = describe({
      kind: "pair",
      key: "k",
      name: "Juan",
      score: 0.11,
      speakers: [speaker("a", "Speaker 1"), speaker("b", "Speaker 2")],
    });
    expect(detail).toContain(
      "Speaker 1 in “Standup a” and Speaker 2 in “Standup b” sound 11% alike.",
    );
  });
});
