import type { Moment, MomentKind } from "../../lib/ipc";
import { formatTimestamp } from "../transcript/group";

/** How each moment reads in the user's notes; `momentsInNotes` reads them back. */
const LABELS: Record<Exclude<MomentKind, "note">, string> = {
  named: "Mentioned",
  asked: "Asked me",
  marked: "Marked moment",
};

/** `- [12:04] Asked me: “Ana, can you…”` — a line for the user's notes. */
export function momentLine(m: Moment) {
  const at = `[${formatTimestamp(m.at)}]`;
  if (m.kind === "note") return `- ${at} ${m.text}`;
  if (m.kind === "marked") return `- ${at} ${LABELS.marked}`;
  return `- ${at} ${LABELS[m.kind]}: “${m.text}”`;
}

/** Adds the moment's line at the end of the notes, on a line of its own. */
export function appendMoment(notes: string, m: Moment) {
  const body = notes.replace(/\s+$/, "");
  return `${body ? `${body}\n` : ""}${momentLine(m)}\n`;
}

/** A moment found in saved notes, for the player's timeline and the transcript. */
export type Mark = { at: number; kind: MomentKind; label: string };

const LINE = /^\s*[-*]\s+\[(\d{1,2}):(\d{2})(?::(\d{2}))?\]\s+(.*)$/;

/** Timestamped lines under "## My notes", which is where moments end up. */
export function momentsInNotes(notes: string): Mark[] {
  const at = notes.indexOf("## My notes");
  if (at === -1) return [];
  return notes
    .slice(at)
    .split("\n")
    .flatMap((line) => {
      const m = line.match(LINE);
      if (!m) return [];
      const [a, b, c] = [m[1], m[2], m[3]].map(Number);
      const seconds = m[3] === undefined ? a * 60 + b : a * 3600 + b * 60 + c;
      const text = m[4].trim();
      const kind: MomentKind = text.startsWith(`${LABELS.asked}:`)
        ? "asked"
        : text.startsWith(`${LABELS.named}:`)
          ? "named"
          : text === LABELS.marked
            ? "marked"
            : "note";
      return [{ at: seconds * 1000, kind, label: text }];
    });
}

type Modifier = "ctrlKey" | "altKey" | "shiftKey" | "metaKey";

const MODIFIERS: [Modifier, string, string][] = [
  ["ctrlKey", "Ctrl", "⌃"],
  ["altKey", "Alt", "⌥"],
  ["shiftKey", "Shift", "⇧"],
  ["metaKey", "Cmd", "⌘"],
];

/** "Ctrl+Alt+M" from a key press, or null until a key with a modifier is pressed. */
export function shortcutFromEvent(e: Pick<KeyboardEvent, Modifier | "code">) {
  const key = e.code.match(/^(?:Key|Digit)(\w)$/)?.[1] ?? e.code.match(/^(F\d{1,2})$/)?.[1];
  const mods = MODIFIERS.filter(([flag]) => e[flag]).map(([, name]) => name);
  if (!key || mods.length === 0) return null;
  return [...mods, key].join("+");
}

/** "Ctrl+Alt+M" → "⌃⌥M", as macOS menus write it. */
export function shortcutLabel(shortcut: string) {
  if (!shortcut.trim()) return "Off";
  return shortcut
    .split("+")
    .map((part) => {
      const mod = MODIFIERS.find(
        ([, name]) =>
          name.toLowerCase() === part.toLowerCase() ||
          (part.toLowerCase() === "control" && name === "Ctrl") ||
          (["option", "opt"].includes(part.toLowerCase()) && name === "Alt") ||
          (["command", "super"].includes(part.toLowerCase()) && name === "Cmd"),
      );
      return mod ? mod[2] : part.toUpperCase();
    })
    .join("");
}
