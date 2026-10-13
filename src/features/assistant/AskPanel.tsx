import { useQuery } from "@tanstack/react-query";
import { ArrowUp, BotMessageSquare, RotateCcw, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ipc } from "../../lib/ipc";
import { Markdown } from "./Markdown";
import { type AskTarget, useAsk } from "./store";

/** Where Ask and notes run, e.g. "Local · qwen3:14b", so cloud use is always visible. */
export function ProviderBadge() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  if (!info.data) return null;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[12px] font-medium ${
        info.data.local ? "bg-local-soft text-on-local-soft" : "bg-accent-soft text-on-accent-soft"
      }`}
      title={info.data.local ? "Runs on this Mac" : "Sent to a cloud API"}
    >
      <span className={`size-1.5 rounded-full ${info.data.local ? "bg-local" : "bg-accent"}`} />
      {info.data.llm}
    </span>
  );
}

const SUGGESTIONS = [
  "What was decided?",
  "List action items so far",
  "Summarize the last 5 minutes",
];

export function AskPanel({
  target,
  onSeek,
  disabled,
  grounding = "Grounded in the transcript",
}: {
  target: AskTarget;
  onSeek: (ms: number) => void;
  /** Why asking isn't possible yet, e.g. nothing transcribed. */
  disabled?: string;
  grounding?: string;
}) {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  const thread = useAsk((s) => s.threads[target]) ?? [];
  const ask = useAsk((s) => s.ask);
  const clear = useAsk((s) => s.clear);
  const [question, setQuestion] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const bottom = useRef<HTMLDivElement>(null);
  const streaming = thread.some((e) => e.status === "streaming");

  // ⌘J jumps to the question box from anywhere (⌘K is search).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "j") {
        e.preventDefault();
        input.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const last = thread.at(-1);
  // biome-ignore lint/correctness/useExhaustiveDependencies: follow the answer as it streams in
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [thread.length, last?.answer]);

  const submit = (text: string) => {
    // "/btw" is how people ask asides in chat; accept it as a prefix.
    const q = text.replace(/^\/btw\s*/i, "").trim();
    if (!q || streaming || disabled) return;
    ask(target, q);
    setQuestion("");
  };

  return (
    <section className="card flex min-h-0 flex-1 flex-col">
      <header className="flex items-start gap-3 px-5 pt-5">
        <span className="rounded-lg bg-accent-soft p-1.5 text-on-accent-soft">
          <BotMessageSquare className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="font-serif text-xl leading-6">Note Copilot</h2>
          {info.data && (
            <p className="truncate font-serif text-[15px] text-ink-soft">
              {info.data.local ? "Offline" : "Cloud"}{" "}
              {info.data.llm.replace(/^(Local|Cloud) · /, "")}
            </p>
          )}
        </div>
        {thread.length > 0 && (
          <button
            type="button"
            className="btn-quiet"
            title="Clear conversation"
            onClick={() => clear(target)}
          >
            <RotateCcw className="size-4" />
          </button>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4 select-text">
        {thread.length === 0 ? (
          <div className="flex flex-col gap-3">
            <span className="eyebrow">Suggested inquiries</span>
            <div className="flex flex-wrap gap-2">
              {SUGGESTIONS.map((s) => (
                <button
                  key={s}
                  type="button"
                  disabled={Boolean(disabled)}
                  className="rounded-lg bg-muted px-3 py-1.5 text-left text-[13px] font-medium hover:bg-high disabled:opacity-50"
                  onClick={() => submit(s)}
                >
                  {s}
                </button>
              ))}
            </div>
            {disabled && <p className="text-[13px] text-ink-soft">{disabled}</p>}
          </div>
        ) : (
          <div className="flex flex-col gap-5">
            {thread.map((e) => (
              <div key={e.id} className="flex flex-col gap-3 text-[14px]">
                <div className="ml-8 self-end rounded-xl bg-muted px-3.5 py-2.5">
                  <p className="text-[12px] text-ink-soft">You</p>
                  <p>{e.question}</p>
                </div>
                <div className="flex flex-col gap-2">
                  <p className="flex items-center gap-1.5 text-[12px] font-semibold text-accent">
                    <Sparkles className="size-3.5" /> Copilot
                  </p>
                  {e.answer && <Markdown text={e.answer} onSeek={onSeek} />}
                  {e.status === "streaming" && !e.answer && (
                    <p className="text-ink-soft">Reading the transcript…</p>
                  )}
                  {e.status === "error" && <p className="text-accent">{e.error}</p>}
                </div>
              </div>
            ))}
          </div>
        )}
        <div ref={bottom} />
      </div>

      <form
        className="px-4 pb-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit(question);
        }}
      >
        <div className="flex items-end gap-2 rounded-xl bg-muted p-2 focus-within:ring-1 focus-within:ring-accent/40">
          <textarea
            ref={input}
            rows={1}
            className="max-h-32 min-h-8 flex-1 resize-none bg-transparent px-2 py-1.5 text-[14px] outline-none placeholder:text-ink-soft field-sizing-content"
            placeholder="Ask anything about this meeting…"
            disabled={Boolean(disabled)}
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit(question);
              }
            }}
          />
          <button
            type="submit"
            aria-label="Ask"
            className="rounded-lg bg-accent p-2 text-on-accent disabled:opacity-40"
            disabled={!question.trim() || streaming || Boolean(disabled)}
          >
            <ArrowUp className="size-4" />
          </button>
        </div>
        <div className="mt-2 flex items-center justify-between px-1 text-[12px] text-ink-soft">
          <span>{grounding}</span>
          <span className="kbd">Enter ↵</span>
        </div>
      </form>
    </section>
  );
}
