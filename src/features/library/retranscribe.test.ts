import { describe, expect, it } from "vitest";
import { estimate } from "./RetranscribeDialog";
import { progressPercent } from "./retranscribe";

describe("re-transcription", () => {
  it("estimates about a fifth of the meeting's length", () => {
    expect(estimate(14 * 60_000)).toBe("about 3 minutes");
    expect(estimate(2 * 60_000)).toBe("about 1 minute");
  });

  it("reports progress in whole percent", () => {
    const job = { status: "running" as const, message: "", done: 0, total: 0 };
    expect(progressPercent(job)).toBe(0);
    expect(progressPercent({ ...job, done: 1, total: 3 })).toBe(33);
  });
});
