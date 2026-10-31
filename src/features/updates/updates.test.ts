import { describe, expect, it } from "vitest";
import { installLabel } from "./updates";

describe("installLabel", () => {
  it("shows download progress", () => {
    expect(installLabel({ status: "downloading", fraction: 0.416 })).toBe("Downloading… 42%");
    expect(installLabel({ status: "downloading", fraction: null })).toBe("Downloading…");
  });

  it("offers the update otherwise", () => {
    expect(installLabel({ status: "idle" })).toBe("Update and restart");
    expect(installLabel({ status: "failed", message: "x" }, "Try again")).toBe("Try again");
  });
});
