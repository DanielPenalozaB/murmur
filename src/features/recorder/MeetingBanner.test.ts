import { describe, expect, it } from "vitest";
import { meetingTitle } from "./MeetingBanner";

describe("meetingTitle", () => {
  const now = new Date(2026, 9, 28, 15, 30);

  it("names the calling app", () => {
    expect(meetingTitle({ app: "Microsoft Teams", browser: false, ended: false }, now)).toMatch(
      /^Microsoft Teams meeting Oct 28, /,
    );
  });

  it("stays generic for a browser", () => {
    expect(meetingTitle({ app: "Google Chrome", browser: true, ended: false }, now)).toMatch(
      /^Meeting Oct 28, /,
    );
  });
});
