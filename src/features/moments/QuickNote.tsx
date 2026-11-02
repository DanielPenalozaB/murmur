import { NotebookPen } from "lucide-react";
import { useState } from "react";
import { ipc } from "../../lib/ipc";

/** A one-line box over the call: Enter adds the note to the meeting at the moment the
 * shortcut was pressed; Escape (or clicking away) closes it. */
export function QuickNote() {
  const [text, setText] = useState("");
  const submit = (value: string) => void ipc.submitQuickNote(value);
  return (
    <form
      className="flex h-screen items-center gap-2.5 rounded-xl bg-surface px-3 text-ink ring-1 ring-accent/40 ring-inset"
      onSubmit={(e) => {
        e.preventDefault();
        submit(text);
      }}
    >
      <NotebookPen className="size-4 shrink-0 text-accent" />
      <input
        // biome-ignore lint/a11y/noAutofocus: the shortcut opens this box to type in
        autoFocus
        className="min-w-0 flex-1 bg-transparent text-[14px] outline-none placeholder:text-outline"
        placeholder="Quick note for this moment…"
        aria-label="Quick note"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && submit("")}
        onBlur={() => submit(text)}
      />
      <kbd className="kbd shrink-0">↵</kbd>
    </form>
  );
}
