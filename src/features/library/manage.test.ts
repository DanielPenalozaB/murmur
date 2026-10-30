import { describe, expect, it } from "vitest";
import { renamedPath } from "./manage";

describe("renamedPath", () => {
  it("keeps the folder in its parent", () => {
    expect(renamedPath("Work", " Team ")).toBe("Team");
    expect(renamedPath("Work/1-1s", "Weekly")).toBe("Work/Weekly");
  });
});
