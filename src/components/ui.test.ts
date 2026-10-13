import { describe, expect, it } from "vitest";
import { avatarTone, initials } from "./ui";

describe("avatars", () => {
  it("makes initials from names", () => {
    expect(initials("Elena Rostova")).toBe("ER");
    expect(initials("Speaker 2")).toBe("S2");
    expect(initials("Me")).toBe("ME");
    expect(initials("  ")).toBe("?");
    expect(initials("Ana María López")).toBe("AL");
  });

  it("gives a name the same tone every time", () => {
    expect(avatarTone("Ana")).toBe(avatarTone("Ana"));
    expect(avatarTone("Me")).toContain("bg-accent");
  });
});
