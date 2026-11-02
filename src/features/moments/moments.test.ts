import { describe, expect, it } from "vitest";
import {
  appendMoment,
  momentLine,
  momentsInNotes,
  shortcutFromEvent,
  shortcutLabel,
} from "./moments";

describe("moment lines", () => {
  it("write each kind for the user's notes", () => {
    expect(momentLine({ kind: "asked", at: 724_000, text: "Ana, can you send it?" })).toBe(
      "- [12:04] Asked me: “Ana, can you send it?”",
    );
    expect(momentLine({ kind: "named", at: 5_000, text: "As Ana said" })).toBe(
      "- [00:05] Mentioned: “As Ana said”",
    );
    expect(momentLine({ kind: "marked", at: 61_000, text: "" })).toBe("- [01:01] Marked moment");
    expect(momentLine({ kind: "note", at: 61_000, text: "check the budget" })).toBe(
      "- [01:01] check the budget",
    );
  });

  it("go on a line of their own after what's typed", () => {
    const m = { kind: "marked", at: 0, text: "" } as const;
    expect(appendMoment("", m)).toBe("- [00:00] Marked moment\n");
    expect(appendMoment("my point  \n\n", m)).toBe("my point\n- [00:00] Marked moment\n");
  });

  it("are read back from saved notes", () => {
    const notes = `## Summary
- Launch Friday [00:34]

## My notes

check rollback
- [12:04] Asked me: “Ana, can you send it?”
- [01:01] Marked moment
- [1:02:03] check the budget`;
    expect(momentsInNotes(notes)).toEqual([
      { at: 724_000, kind: "asked", label: "Asked me: “Ana, can you send it?”" },
      { at: 61_000, kind: "marked", label: "Marked moment" },
      { at: 3_723_000, kind: "note", label: "check the budget" },
    ]);
    expect(momentsInNotes("## Summary\n- [00:34] x")).toEqual([]);
  });
});

describe("shortcuts", () => {
  const press = (code: string, mods: Partial<Record<string, boolean>> = {}) => ({
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    code,
    ...mods,
  });

  it("are recorded from a key press with a modifier", () => {
    expect(shortcutFromEvent(press("KeyM", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+M");
    expect(shortcutFromEvent(press("Digit5", { metaKey: true, shiftKey: true }))).toBe(
      "Shift+Cmd+5",
    );
    expect(shortcutFromEvent(press("KeyM"))).toBeNull();
    expect(shortcutFromEvent(press("ControlLeft", { ctrlKey: true }))).toBeNull();
  });

  it("are shown with the macOS symbols", () => {
    expect(shortcutLabel("Ctrl+Alt+M")).toBe("⌃⌥M");
    expect(shortcutLabel("control+option+n")).toBe("⌃⌥N");
    expect(shortcutLabel("Shift+Cmd+5")).toBe("⇧⌘5");
    expect(shortcutLabel("")).toBe("Off");
  });
});
