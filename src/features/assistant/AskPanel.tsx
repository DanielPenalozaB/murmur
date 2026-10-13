import { useQuery } from "@tanstack/react-query";
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
      className={`rounded-full px-2 py-0.5 text-xs ${
        info.data.local ? "bg-local/10 text-local" : "bg-accent/10 text-accent"
      }`}
      title={info.data.local ? "Runs on this Mac" : "Sent to a cloud API"}
    >
      {info.data.llm}
    </span>
  );
}

const EXAMPLES = [
  "What did they decide?",
  "What are my action items?",
  "Summarize the last 5 minutes",
];

export function AskPanel({
  target,
  onSeek,
  disabled,
}: {
  target: AskTarget;
  onSeek: (ms: number) => void;
  /** Why asking isn't possible yet, e.g. nothing transcribed. */
  disabled?: string;
}) {
  const thread = useAsk((s) => s.threads[target]) ?? [];
  const ask = useAsk((s) => s.ask);
  const [question, setQuestion] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const bottom = useRef<HTMLDivElement>(null);
  const streaming = thread.some((e) => e.status === "streaming");

  // ⌘K jumps to the question box from anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "k") {
        e.preventDefault();
        input.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const last = thread.at(-1);
  // biome-ignore lint/correctness/useExhaustiveDependencies: follow the answer as it streams in
  useEffect(() => bottom.current?.scrollIntoView({ block: "end" }), [thread.length, last?.answer]);

  const submit = (text: string) => {
    // "/btw" is how people ask asides in chat; accept it as a prefix.
    const q = text.replace(/^\/btw\s*/i, "").trim();
    if (!q || streaming || disabled) return;
    ask(target, q);
    setQuestion("");
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex-1 overflow-y-auto px-4 py-4 select-text">
        {thread.length === 0 ? (
          <div className="flex flex-col gap-2 text-sm text-ink-soft">
            <p>Ask anything about this meeting. Answers cite the transcript.</p>
            {!disabled &&
              EXAMPLES.map((example) => (
                <button
                  key={example}
                  type="button"
                  className="rounded-lg border border-line px-3 py-1.5 text-left hover:bg-muted"
                  onClick={() => submit(example)}
                >
                  {example}
                </button>
              ))}
          </div>
        ) : (
          <div className="flex flex-col gap-5">
            {thread.map((e) => (
              <div key={e.id} className="flex flex-col gap-2 text-sm">
                <p className="self-end rounded-xl bg-muted px-3 py-2">{e.question}</p>
                {e.answer && <Markdown text={e.answer} onSeek={onSeek} />}
                {e.status === "streaming" && !e.answer && (
                  <p className="text-ink-soft">Thinking…</p>
                )}
                {e.status === "error" && <p className="text-accent">{e.error}</p>}
              </div>
            ))}
          </div>
        )}
        <div ref={bottom} />
      </div>
      <form
        className="border-t border-line p-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit(question);
        }}
      >
        <textarea
          ref={input}
          rows={2}
          className="input w-full resize-none"
          placeholder={disabled ?? "Ask about the meeting…  ⌘K"}
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
        <div className="mt-2 flex items-center justify-between">
          <ProviderBadge />
          <button type="submit" className="btn-primary" disabled={!question.trim() || streaming}>
            Ask
          </button>
        </div>
      </form>
    </div>
  );
}
