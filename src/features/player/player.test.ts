import { describe, expect, it } from "vitest";
import { nextRate, skip } from "./player";

describe("player", () => {
  it("skips within the recording", () => {
    expect(skip(10_000, -15_000, 60_000)).toBe(0);
    expect(skip(10_000, 15_000, 60_000)).toBe(25_000);
    expect(skip(55_000, 15_000, 60_000)).toBe(60_000);
  });

  it("cycles through the speeds", () => {
    expect(nextRate(1)).toBe(1.25);
    expect(nextRate(2)).toBe(1);
    expect(nextRate(3)).toBe(1);
  });
});
