import { describe, expect, it } from "vitest";
import { folderCrumbs } from "./nav";

describe("folderCrumbs", () => {
  it("links the vault and each folder level", () => {
    expect(folderCrumbs("Work/1-1s")).toEqual([
      { label: "Vault", to: { kind: "folder", folder: null } },
      { label: "Work", to: { kind: "folder", folder: "Work" } },
      { label: "1-1s", to: { kind: "folder", folder: "Work/1-1s" } },
    ]);
    expect(folderCrumbs("")).toHaveLength(1);
  });
});
