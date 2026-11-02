import { describe, expect, it } from "vitest";
import { eachInTurn, renamedPath } from "./manage";

describe("renamedPath", () => {
  it("keeps the folder in its parent", () => {
    expect(renamedPath("Work", " Team ")).toBe("Team");
    expect(renamedPath("Work/1-1s", "Weekly")).toBe("Work/Weekly");
  });
});

describe("eachInTurn", () => {
  it("keeps going past failures and reports them in order", async () => {
    const seen: number[] = [];
    const failed = await eachInTurn([1, 2, 3, 4], async (n) => {
      seen.push(n);
      if (n % 2 === 0) throw new Error(`no ${n}`);
    });
    expect(seen).toEqual([1, 2, 3, 4]);
    expect(failed).toEqual([
      { item: 2, error: "Error: no 2" },
      { item: 4, error: "Error: no 4" },
    ]);
  });
});
