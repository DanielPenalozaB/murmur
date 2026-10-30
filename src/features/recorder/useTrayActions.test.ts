import { describe, expect, it } from "vitest";
import type { Settings } from "../../lib/ipc";
import { traySession } from "./useTrayActions";

const settings = {
  last_folder: "Work",
  default_format: "markdown",
  languages: ["en", "es"],
} as Settings;
const now = new Date(2026, 9, 28, 15, 30);

describe("traySession", () => {
  it("reuses the last folder, format and languages", () => {
    expect(traySession(settings, null, now)).toMatchObject({
      folder: "Work",
      format: "markdown",
      languages: ["en", "es"],
    });
    expect(traySession(settings, null, now).title).toMatch(/^Meeting Oct 28, /);
  });

  it("names a detected call", () => {
    expect(
      traySession(settings, { app: "Slack", browser: false, ended: false }, now).title,
    ).toMatch(/^Slack meeting Oct 28, /);
  });
});
