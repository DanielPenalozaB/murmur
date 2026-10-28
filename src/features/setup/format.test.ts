import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("rounds to MB, or GB above a gigabyte", () => {
    expect(formatBytes(769_208_387)).toBe("769 MB");
    expect(formatBytes(1_234_000_000)).toBe("1.2 GB");
    expect(formatBytes(10)).toBe("1 MB");
  });
});
