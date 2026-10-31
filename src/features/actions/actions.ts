import type { TaskNotes } from "../../lib/ipc";
import { actionItems } from "../assistant/notes";

/** One action item, wherever it was agreed. */
export type Task = {
  /** Unique across meetings: session and line. */
  key: string;
  sessionId: string;
  title: string;
  folder: string;
  startedAt: string;
  hasAudio: boolean;
  /** Line in the session's notes, for ticking it. */
  line: number;
  done: boolean;
  owners: string[];
  mine: boolean;
  /** The task without its timestamp or due-date remark. */
  text: string;
  /** When it was said, in ms from the start of the meeting. */
  at: number | null;
  /** Local date, `YYYY-MM-DD`. */
  due: string | null;
};

export type Group = "overdue" | "week" | "later" | "undated";

const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();

/** "Ana and Bo", "Ana, Bo y Cy" → each person. */
export function splitOwners(owner: string | null) {
  if (!owner) return [];
  return owner
    .split(/\s*(?:,|&|\band\b|\by\b)\s*/i)
    .map((o) => o.trim())
    .filter(Boolean);
}

/** "Me", or the user's name: in full, or the first name on its own. */
export function isMine(owner: string, userName: string) {
  const o = fold(owner);
  if (["me", "yo", "you"].includes(o)) return true;
  const name = fold(userName);
  if (!name) return false;
  return o === name || o === name.split(/\s+/)[0];
}

const STAMP = /\s*[[(](\d{1,2}):(\d{2})(?::(\d{2}))?(?:\s*[,–-][^\])]*)?[\])]/g;

/** The first `[mm:ss]` or `(h:mm:ss)` citation, in ms. */
export function timestampOf(text: string) {
  const m = [...text.matchAll(STAMP)][0];
  if (!m) return null;
  const [a, b, c] = [m[1], m[2], m[3]].map((n) => (n === undefined ? undefined : Number(n)));
  const seconds =
    c === undefined
      ? (a as number) * 60 + (b as number)
      : (a as number) * 3600 + (b as number) * 60 + c;
  return seconds * 1000;
}

const WEEKDAYS: Record<string, number> = {
  sunday: 0,
  domingo: 0,
  monday: 1,
  lunes: 1,
  tuesday: 2,
  martes: 2,
  wednesday: 3,
  miercoles: 3,
  thursday: 4,
  jueves: 4,
  friday: 5,
  viernes: 5,
  saturday: 6,
  sabado: 6,
};

const MONTHS: [RegExp, number][] = [
  [/^(jan(uary)?|ene(ro)?)$/, 0],
  [/^(feb(ruary)?|feb(rero)?)$/, 1],
  [/^(mar(ch)?|mar(zo)?)$/, 2],
  [/^(apr(il)?|abr(il)?)$/, 3],
  [/^(may|mayo)$/, 4],
  [/^(jun(e)?|jun(io)?)$/, 5],
  [/^(jul(y)?|jul(io)?)$/, 6],
  [/^(aug(ust)?|ago(sto)?)$/, 7],
  [/^(sep(t(ember)?)?|sep(tiembre)?|set(iembre)?)$/, 8],
  [/^(oct(ober)?|oct(ubre)?)$/, 9],
  [/^(nov(ember)?|nov(iembre)?)$/, 10],
  [/^(dec(ember)?|dic(iembre)?)$/, 11],
];

const month = (word: string) => MONTHS.find(([re]) => re.test(word))?.[1];

const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);

export function isoDay(d: Date) {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** A due date in a phrase, counted from the meeting's day. */
export function parseDue(phrase: string, meeting: Date): Date | null {
  const text = fold(phrase);
  if (/not specified|no due|sin fecha|tbd|n\/a/.test(text)) return null;
  const day = new Date(meeting.getFullYear(), meeting.getMonth(), meeting.getDate());
  const words = text.split(/[^a-z0-9]+/).filter(Boolean);

  // "Nov 5", "November 5th", "5 de noviembre", "5 November".
  for (let i = 0; i < words.length; i++) {
    const m = month(words[i]);
    if (m === undefined) continue;
    const after = Number.parseInt(words[i + 1] ?? "", 10);
    const before = Number.parseInt(words[i - 1] === "de" ? words[i - 2] : (words[i - 1] ?? ""), 10);
    const date = Number.isNaN(after) ? before : after;
    if (Number.isNaN(date) || date < 1 || date > 31) continue;
    let due = new Date(day.getFullYear(), m, date);
    // Months well before the meeting mean next year ("Jan 5" in a December meeting).
    if (due < addDays(day, -60)) due = new Date(day.getFullYear() + 1, m, date);
    return due;
  }
  for (const w of words) {
    const weekday = WEEKDAYS[w];
    if (weekday !== undefined) return addDays(day, ((weekday - day.getDay() + 6) % 7) + 1);
  }
  if (/\b(today|hoy|eod|end of (the )?day)\b/.test(text)) return day;
  if (/\b(tomorrow|manana)\b/.test(text)) return addDays(day, 1);
  if (/\bend of (the |this )?week\b|\bfin de (la )?semana\b/.test(text)) {
    return addDays(day, (5 - day.getDay() + 7) % 7);
  }
  if (/\bnext week\b|\b(proxima|siguiente) semana\b|\bsemana que viene\b/.test(text)) {
    return addDays(day, ((1 - day.getDay() + 6) % 7) + 1);
  }
  // "the 5th", "el 5".
  const nth = text.match(/\b(?:the |el )?(\d{1,2})(?:st|nd|rd|th)\b|\bel (\d{1,2})\b/);
  if (nth) {
    const date = Number(nth[1] ?? nth[2]);
    if (date >= 1 && date <= 31) {
      const due = new Date(day.getFullYear(), day.getMonth(), date);
      return due < day ? new Date(day.getFullYear(), day.getMonth() + 1, date) : due;
    }
  }
  return null;
}

// "(due Friday)", "(by Nov 5)", "(due date: 5th)", "(due date not specified)".
const REMARK = /\s*\((?:due(?: date)?:?|by|para(?: el)?|antes del?|deadline:?)?\s*([^()]*)\)/gi;
const INLINE =
  /\b(?:by|before|para el|antes del?)\s+((?:next |the |el |end of (?:the )?)?[\p{L}\d]+(?:\s+\d{1,2}(?:st|nd|rd|th)?)?)/iu;

/** Finds the due date and the text without timestamps or due remarks. */
export function readTask(text: string, meeting: Date) {
  let due: Date | null = null;
  let clean = text.replace(STAMP, "");
  clean = clean.replace(REMARK, (whole, inside: string) => {
    const found = parseDue(inside, meeting);
    const remark = /due|not specified|sin fecha|deadline/i.test(whole);
    if (found && !due) due = found;
    return found || remark ? "" : whole;
  });
  if (!due) {
    const inline = clean.match(INLINE);
    if (inline) due = parseDue(inline[1], meeting);
  }
  clean = clean
    .replace(/\s+/g, " ")
    .replace(/\s+([.,;])/g, "$1")
    .replace(/[.;,]\s*$/, "")
    .trim();
  return { text: clean, due: due as Date | null };
}

/** Every action item in these meetings, newest meeting first. */
export function collectTasks(sources: TaskNotes[], userName: string): Task[] {
  return sources.flatMap((s) => {
    const meeting = new Date(s.started_at);
    return actionItems(s.notes).map((item) => {
      const owners = splitOwners(item.owner);
      const { text, due } = readTask(item.text, meeting);
      return {
        key: `${s.id}:${item.line}`,
        sessionId: s.id,
        title: s.title,
        folder: s.folder,
        startedAt: s.started_at,
        hasAudio: s.has_audio,
        line: item.line,
        done: item.done,
        owners,
        mine: owners.some((o) => isMine(o, userName)),
        text,
        at: timestampOf(item.text),
        due: due ? isoDay(due) : null,
      };
    });
  });
}

/** Where an open task goes, by its due date. */
export function groupOf(task: Task, today: Date): Group {
  if (!task.due) return "undated";
  const t = isoDay(today);
  if (task.due < t) return "overdue";
  return task.due <= isoDay(addDays(today, 7)) ? "week" : "later";
}

/** "Today", "Tomorrow", "Yesterday", "Fri, Nov 6". */
export function dueLabel(due: string, today: Date) {
  const [y, m, d] = due.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const days = Math.round(
    (date.getTime() - new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()) /
      86_400_000,
  );
  if (days === 0) return "Today";
  if (days === 1) return "Tomorrow";
  if (days === -1) return "Yesterday";
  return date.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
}
