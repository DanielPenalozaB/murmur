import { describe, expect, it } from "vitest";
import { linkCitations, parseTimestamp } from "./citations";

describe("citations", () => {
  it("parses mm:ss and h:mm:ss", () => {
    expect(parseTimestamp("12:04")).toBe(724_000);
    expect(parseTimestamp("1:02:03")).toBe(3_723_000);
  });

  it("links single, listed and ranged citations", () => {
    expect(linkCitations("Ship it [00:31].")).toBe("Ship it [00:31](#t=31000).");
    expect(linkCitations("[00:31, 00:39]")).toBe("[00:31](#t=31000), [00:39](#t=39000)");
    expect(linkCitations("[12:04–12:30]")).toBe("[12:04](#t=724000)–[12:30](#t=750000)");
  });

  it("leaves other brackets alone", () => {
    expect(linkCitations("- [ ] Ana: send the plan")).toBe("- [ ] Ana: send the plan");
    expect(linkCitations("[link](https://x.y)")).toBe("[link](https://x.y)");
  });
});
