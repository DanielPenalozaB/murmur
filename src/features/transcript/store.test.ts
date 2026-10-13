import { beforeEach, describe, expect, it } from "vitest";
import type { Segment } from "../../lib/ipc";
import { useTranscript } from "./store";

const seg = (start: number, channel: Segment["channel"], text: string): Segment => ({
  start,
  end: start + 1000,
  channel,
  speaker: null,
  text,
});

describe("live transcript store", () => {
  beforeEach(() => useTranscript.getState().begin("Test"));

  it("replaces a draft when its final line arrives", () => {
    const { handle } = useTranscript.getState();
    handle({ kind: "draft", channel: "mic", start: 1000, text: "hel" });
    handle({ kind: "draft", channel: "mic", start: 1000, text: "hello wor" });
    expect(useTranscript.getState().drafts).toEqual([
      { channel: "mic", start: 1000, text: "hello wor" },
    ]);
    handle({ kind: "segment", segment: seg(1000, "mic", "Hello world.") });
    handle({ kind: "draft_done", channel: "mic", until: 3000 });
    expect(useTranscript.getState().drafts).toEqual([]);
  });

  it("ignores drafts of audio that's already final", () => {
    const { handle } = useTranscript.getState();
    handle({ kind: "draft_done", channel: "system", until: 5000 });
    handle({ kind: "draft", channel: "system", start: 4000, text: "late" });
    handle({ kind: "draft", channel: "mic", start: 4000, text: "other channel" });
    expect(useTranscript.getState().drafts.map((d) => d.text)).toEqual(["other channel"]);
  });

  it("swaps in speaker-labelled lines when the meeting is saved", () => {
    const { handle } = useTranscript.getState();
    handle({ kind: "segment", segment: seg(0, "system", "Hi.") });
    handle({ kind: "status", message: "Identifying speakers…" });
    expect(useTranscript.getState().status).toBe("Identifying speakers…");
    const labelled = { ...seg(0, "system", "Hi."), speaker: "speaker_1" };
    handle({
      kind: "finished",
      session: null,
      segments: [labelled],
      speakers: [{ id: "speaker_1", name: "Speaker 1", named: false }],
    });
    const state = useTranscript.getState();
    expect(state.segments).toEqual([labelled]);
    expect(state.speakers).toHaveLength(1);
    expect(state.status).toBeNull();
  });
});
