import { describe, expect, it } from "vitest";
import { denied } from "./PermissionNotice";

describe("denied", () => {
  it("lists only refused sources", () => {
    expect(denied({ mic: "granted", system: "denied" }).map((s) => s.source)).toEqual(["system"]);
    expect(denied({ mic: "undetermined", system: "unknown" })).toEqual([]);
    expect(denied(null)).toEqual([]);
  });
});
