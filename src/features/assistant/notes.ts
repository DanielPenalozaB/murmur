/** A `## Title` section of the notes Markdown. */
export type Section = { title: string; body: string };

/** Splits notes into the text before the first `## ` heading and its sections. */
export function parseSections(notes: string): { intro: string; sections: Section[] } {
  const lines = notes.split("\n");
  const intro: string[] = [];
  const sections: Section[] = [];
  for (const line of lines) {
    const heading = line.match(/^##\s+(.+?)\s*$/);
    if (heading) sections.push({ title: heading[1], body: "" });
    else if (sections.length > 0) {
      const last = sections[sections.length - 1];
      last.body += (last.body ? "\n" : "") + line;
    } else intro.push(line);
  }
  for (const s of sections) s.body = s.body.trim();
  return { intro: intro.join("\n").trim(), sections };
}

export type ActionItem = {
  /** Line number in the full notes text, for toggling. */
  line: number;
  done: boolean;
  owner: string | null;
  text: string;
};

const TASK = /^(\s*[-*]\s+)\[([ xX])\]\s+(.*)$/;

/** Task-list items anywhere in the notes, e.g. `- [ ] Ana: send the plan [03:10]`. */
export function actionItems(notes: string): ActionItem[] {
  return notes.split("\n").flatMap((raw, line) => {
    const m = raw.match(TASK);
    if (!m) return [];
    const body = m[3].trim();
    // "Who: task" with a short "who" (a name, not a sentence that happens to have a colon).
    const owner = body.match(/^([^:[\]]{1,40}):\s+(.+)$/);
    return [
      {
        line,
        done: m[2] !== " ",
        owner: owner ? owner[1].trim() : null,
        text: owner ? owner[2] : body,
      },
    ];
  });
}

/** Flips one task's checkbox and returns the new notes text. */
export function toggleAction(notes: string, line: number) {
  const lines = notes.split("\n");
  const m = lines[line]?.match(TASK);
  if (!m) return notes;
  lines[line] = `${m[1]}[${m[2] === " " ? "x" : " "}] ${m[3]}`;
  return lines.join("\n");
}

/** Top-level bullets of a section, for rendering each as a card. */
export function bullets(body: string): string[] {
  const items: string[] = [];
  for (const line of body.split("\n")) {
    const m = line.match(/^[-*]\s+(.*)$/) ?? line.match(/^\d+[.)]\s+(.*)$/);
    if (m) items.push(m[1]);
    else if (line.trim() && items.length > 0) items[items.length - 1] += ` ${line.trim()}`;
  }
  return items;
}
