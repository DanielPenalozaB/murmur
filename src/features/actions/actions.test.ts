import { describe, expect, it } from "vitest";
import type { TaskNotes } from "../../lib/ipc";
import {
  collectTasks,
  dueLabel,
  groupOf,
  isMine,
  isoDay,
  parseDue,
  readTask,
  splitOwners,
  timestampOf,
} from "./actions";

// A Wednesday.
const meeting = new Date(2026, 9, 28, 10, 30);
const due = (phrase: string) => {
  const d = parseDue(phrase, meeting);
  return d ? isoDay(d) : null;
};

describe("owners", () => {
  it("splits several people", () => {
    expect(splitOwners("Ana and Bo Ruiz")).toEqual(["Ana", "Bo Ruiz"]);
    expect(splitOwners("Ana, Bo, Cy & Di")).toEqual(["Ana", "Bo", "Cy", "Di"]);
    expect(splitOwners("Ana y Bo")).toEqual(["Ana", "Bo"]);
    expect(splitOwners(null)).toEqual([]);
  });

  it("knows which are the user's", () => {
    expect(isMine("Me", "")).toBe(true);
    expect(isMine("Ana", "Ana Torres")).toBe(true);
    expect(isMine("ana torres", "Ana Torres")).toBe(true);
    expect(isMine("Ramón", "Ramon Diaz")).toBe(true);
    expect(isMine("Anabel", "Ana Torres")).toBe(false);
    expect(isMine("Ana", "")).toBe(false);
  });
});

describe("timestamps", () => {
  it("reads the first citation", () => {
    expect(timestampOf("Send it [01:44]")).toBe(104_000);
    expect(timestampOf("Send it (03:21).")).toBe(201_000);
    expect(timestampOf("Send it [1:02:03, 1:05:00]")).toBe(3_723_000);
    expect(timestampOf("Send it")).toBeNull();
  });
});

describe("due dates", () => {
  it("counts from the meeting's day", () => {
    expect(due("Friday")).toBe("2026-10-30");
    expect(due("viernes por la mañana")).toBe("2026-10-30");
    expect(due("Wednesday")).toBe("2026-11-04");
    expect(due("today")).toBe("2026-10-28");
    expect(due("mañana")).toBe("2026-10-29");
    expect(due("end of the week")).toBe("2026-10-30");
    expect(due("next week")).toBe("2026-11-02");
    expect(due("la próxima semana")).toBe("2026-11-02");
  });

  it("reads calendar dates", () => {
    expect(due("Nov 5")).toBe("2026-11-05");
    expect(due("November 5th")).toBe("2026-11-05");
    expect(due("5 de noviembre")).toBe("2026-11-05");
    expect(due("the 5th")).toBe("2026-11-05");
    expect(due("the 30th")).toBe("2026-10-30");
    expect(due("Jan 9")).toBe("2027-01-09");
  });

  it("ignores phrases without a date", () => {
    expect(due("due date not specified")).toBeNull();
    expect(due("Speaker 5")).toBeNull();
    expect(due("tickets 108 and 453")).toBeNull();
  });
});

describe("readTask", () => {
  const read = (text: string) => {
    const r = readTask(text, meeting);
    return [r.text, r.due ? isoDay(r.due) : null];
  };

  it("drops timestamps and due remarks from the text", () => {
    expect(read("Update test cases (due date not specified) [01:04]")).toEqual([
      "Update test cases",
      null,
    ]);
    expect(read("Set up the planning meeting (due date: 5th) [02:23]")).toEqual([
      "Set up the planning meeting",
      "2026-11-05",
    ]);
    expect(read("Send the plan (Friday) [12:04].")).toEqual(["Send the plan", "2026-10-30"]);
  });

  it("finds dates said inline and keeps other remarks", () => {
    expect(read("Ship the build by Friday [03:00]")).toEqual([
      "Ship the build by Friday",
      "2026-10-30",
    ]);
    expect(read("Sign off story 408 (Moog, PayPal) [01:11]")).toEqual([
      "Sign off story 408 (Moog, PayPal)",
      null,
    ]);
    expect(read("Ask the team by email")).toEqual(["Ask the team by email", null]);
  });
});

describe("collectTasks", () => {
  const source: TaskNotes = {
    id: "s1",
    title: "Standup",
    folder: "Work",
    started_at: meeting.toISOString(),
    duration_ms: 900_000,
    format: "markdown",
    file_path: "/v/Work/Standup.md",
    has_audio: true,
    notes:
      "## Action items\n- [ ] Me: Write the spec (Friday) [01:44]\n- [x] Ana and Bo: Book the room [02:00]\n- [ ] Review the PR",
  };

  it("reads every task with its line, owners and moment", () => {
    const tasks = collectTasks([source], "Bo Ruiz");
    expect(tasks.map((t) => [t.key, t.done, t.owners, t.mine, t.text, t.at, t.due])).toEqual([
      ["s1:1", false, ["Me"], true, "Write the spec", 104_000, "2026-10-30"],
      ["s1:2", true, ["Ana", "Bo"], true, "Book the room", 120_000, null],
      ["s1:3", false, [], false, "Review the PR", null, null],
    ]);
  });
});

describe("grouping", () => {
  const today = new Date(2026, 9, 30);
  const task = (due: string | null) => ({ due }) as Parameters<typeof groupOf>[0];

  it("sorts open tasks by when they're due", () => {
    expect(groupOf(task("2026-10-29"), today)).toBe("overdue");
    expect(groupOf(task("2026-10-30"), today)).toBe("week");
    expect(groupOf(task("2026-11-06"), today)).toBe("week");
    expect(groupOf(task("2026-11-07"), today)).toBe("later");
    expect(groupOf(task(null), today)).toBe("undated");
  });

  it("names nearby days", () => {
    expect(dueLabel("2026-10-30", today)).toBe("Today");
    expect(dueLabel("2026-10-31", today)).toBe("Tomorrow");
    expect(dueLabel("2026-10-29", today)).toBe("Yesterday");
  });
});
