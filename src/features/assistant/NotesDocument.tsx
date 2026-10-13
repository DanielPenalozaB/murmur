import { CheckCircle2, Sparkles } from "lucide-react";
import { Avatar } from "../../components/ui";
import { Markdown } from "./Markdown";
import { actionItems, bullets, parseSections, type Section } from "./notes";

/** Renders notes Markdown with the Stitch layout: summary callout, numbered decisions,
 *  action items with owners, and the user's own notes as a quote card. */
export function NotesDocument({
  notes,
  onSeek,
  onToggleAction,
}: {
  notes: string;
  onSeek: (ms: number) => void;
  /** Called with the line of a task to flip; omit to make checkboxes read-only. */
  onToggleAction?: (line: number) => void;
}) {
  const { intro, sections } = parseSections(notes);
  const items = actionItems(notes);

  const renderSection = (s: Section) => {
    const key = s.title.toLowerCase();
    if (key === "summary") {
      return (
        <section key={s.title} className="flex flex-col gap-4">
          <div className="flex items-baseline justify-between">
            <h2 className="font-serif text-[28px]">Executive Summary</h2>
            <span className="eyebrow">Murmur synthesis</span>
          </div>
          <div className="flex gap-4 rounded-xl border-l-4 border-accent bg-sidebar p-5">
            <span className="h-fit rounded-lg bg-accent-soft p-1.5 text-on-accent-soft">
              <Sparkles className="size-4" />
            </span>
            <div className="min-w-0 flex-1">
              <Markdown text={s.body} onSeek={onSeek} />
            </div>
          </div>
        </section>
      );
    }
    if (key === "decisions") {
      const list = bullets(s.body);
      return (
        <section key={s.title} className="flex flex-col gap-3">
          <h2 className="font-serif text-2xl">Decisions</h2>
          {list.length === 0 ? (
            <Markdown text={s.body} onSeek={onSeek} />
          ) : (
            list.map((d, i) => (
              <div key={d} className="card flex gap-4 p-4">
                <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-[13px] font-semibold">
                  {i + 1}
                </span>
                <div className="min-w-0 flex-1 pt-0.5">
                  <Markdown text={d} onSeek={onSeek} />
                </div>
              </div>
            ))
          )}
        </section>
      );
    }
    if (key === "action items") {
      const done = items.filter((i) => i.done).length;
      return (
        <section key={s.title} className="flex flex-col gap-3" id="action-items">
          <div className="flex items-baseline justify-between">
            <h2 className="font-serif text-[28px]">Action Items &amp; Ownership</h2>
            {items.length > 0 && (
              <span className="flex items-center gap-1.5 text-[13px] text-ink-soft">
                <CheckCircle2 className="size-4" /> {done} of {items.length} completed
              </span>
            )}
          </div>
          {items.length === 0 ? (
            <Markdown text={s.body} onSeek={onSeek} />
          ) : (
            items.map((item) => (
              <div key={item.line} className="card flex items-center gap-3 px-4 py-3">
                <input
                  type="checkbox"
                  className="size-4 shrink-0 accent-accent"
                  checked={item.done}
                  disabled={!onToggleAction}
                  onChange={() => onToggleAction?.(item.line)}
                  aria-label={item.text}
                />
                <div className={`min-w-0 flex-1 ${item.done ? "text-ink-soft line-through" : ""}`}>
                  <Markdown text={item.text} onSeek={onSeek} />
                </div>
                {item.done && <span className="chip bg-local-soft text-on-local-soft">Done</span>}
                {item.owner && (
                  <span className="chip shrink-0 bg-muted">
                    <Avatar name={item.owner} size="xs" />
                    {item.owner}
                  </span>
                )}
              </div>
            ))
          )}
        </section>
      );
    }
    if (key === "my notes") {
      return (
        <section key={s.title} className="flex flex-col gap-3">
          <h2 className="font-serif text-2xl">Your notes</h2>
          <div className="rounded-xl border-l-2 border-warm bg-sidebar px-5 py-4 font-serif text-[17px] italic whitespace-pre-wrap text-ink-soft">
            {s.body}
          </div>
        </section>
      );
    }
    return (
      <section key={s.title} className="flex flex-col gap-3">
        <h2 className="font-serif text-2xl">{s.title}</h2>
        <Markdown text={s.body} onSeek={onSeek} />
      </section>
    );
  };

  return (
    <div className="flex flex-col gap-10">
      {intro && <Markdown text={intro} onSeek={onSeek} />}
      {sections.map(renderSection)}
    </div>
  );
}
