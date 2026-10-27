import { describe, expect, it } from "vitest";
import { parseVocabulary } from "./vocabulary";

describe("parseVocabulary", () => {
  it("splits on commas and lines and drops blanks and repeats", () => {
    expect(parseVocabulary("TKT, Mariana,\nExpress Checkout\n\n tkt ,")).toEqual([
      "TKT",
      "Mariana",
      "Express Checkout",
    ]);
    expect(parseVocabulary("  ")).toEqual([]);
  });
});
