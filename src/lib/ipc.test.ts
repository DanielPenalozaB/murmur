import { describe, expect, it } from "vitest";
import { formatDuration, formatRelative } from "./ipc";

describe("formatting", () => {
  const now = new Date(2026, 9, 13, 12, 0);

  it("says today and yesterday", () => {
    expect(formatRelative(new Date(2026, 9, 13, 9, 30).toISOString(), now)).toMatch(/^Today, /);
    expect(formatRelative(new Date(2026, 9, 12, 23, 0).toISOString(), now)).toMatch(/^Yesterday, /);
    expect(formatRelative(new Date(2026, 8, 1).toISOString(), now)).toMatch(/2026/);
  });

  it("formats durations", () => {
    expect(formatDuration(20_000)).toBe("<1 min");
    expect(formatDuration(25 * 60_000)).toBe("25 min");
    expect(formatDuration(75 * 60_000)).toBe("1 h 15 min");
  });
});
