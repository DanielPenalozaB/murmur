import { describe, expect, it } from "vitest";
import { actionItems, bullets, parseSections, toggleAction } from "./notes";

const NOTES = `Intro line

## Summary
- Launch moves to Friday [12:04]

## Decisions
1. Ship on Friday
2. Keep the old API
   for one more release

## Action items
- [ ] Ana: send the rollout plan [03:10]
- [x] Book the room
- [ ] Follow up: a sentence that is way too long to be a person's name at all: really

## My notes

ship it`;

describe("notes", () => {
  it("splits sections", () => {
    const { intro, sections } = parseSections(NOTES);
    expect(intro).toBe("Intro line");
    expect(sections.map((s) => s.title)).toEqual([
      "Summary",
      "Decisions",
      "Action items",
      "My notes",
    ]);
    expect(sections[3].body).toBe("ship it");
  });

  it("reads action items with owners", () => {
    const items = actionItems(NOTES);
    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({
      done: false,
      owner: "Ana",
      text: "send the rollout plan [03:10]",
    });
    expect(items[1]).toMatchObject({ done: true, owner: null, text: "Book the room" });
    expect(items[2].owner).toBe("Follow up");
  });

  it("toggles one checkbox and nothing else", () => {
    const [first] = actionItems(NOTES);
    const toggled = toggleAction(NOTES, first.line);
    expect(actionItems(toggled)[0].done).toBe(true);
    expect(toggleAction(toggled, first.line)).toBe(NOTES);
    expect(toggleAction(NOTES, 0)).toBe(NOTES);
  });

  it("collects bullets, joining wrapped lines", () => {
    const decisions = parseSections(NOTES).sections[1].body;
    expect(bullets(decisions)).toEqual(["Ship on Friday", "Keep the old API for one more release"]);
  });
});
