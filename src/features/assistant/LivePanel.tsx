import { useState } from "react";
import { useTranscript } from "../transcript/store";
import { AskPanel } from "./AskPanel";

/** Right column during a meeting: the user's own notes and Ask. */
export function LivePanel({ onSeek }: { onSeek: (ms: number) => void }) {
  const [tab, setTab] = useState<"notes" | "ask">("ask");
  const { userNotes, setUserNotes, segments } = useTranscript();

  return (
    <aside className="flex min-h-0 flex-col border-l border-line bg-sidebar">
      <div className="flex gap-1 border-b border-line px-3 pb-2 pt-10" data-tauri-drag-region>
        {(["ask", "notes"] as const).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={`rounded-md px-3 py-1 text-sm ${tab === t ? "bg-muted font-medium" : "text-ink-soft hover:text-ink"}`}
          >
            {t === "ask" ? "Ask" : "My notes"}
          </button>
        ))}
      </div>
      {tab === "ask" ? (
        <AskPanel
          target="live"
          onSeek={onSeek}
          disabled={
            segments.length === 0 ? "Questions open after the first transcribed line" : undefined
          }
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-2 p-3">
          <textarea
            className="input min-h-0 flex-1 resize-none leading-relaxed"
            placeholder={
              "Jot things down as they happen.\nWhen the recording stops, they're expanded into full notes."
            }
            value={userNotes}
            onChange={(e) => setUserNotes(e.target.value)}
          />
        </div>
      )}
    </aside>
  );
}
