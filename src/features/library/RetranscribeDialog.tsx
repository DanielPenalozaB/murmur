import { useQuery } from "@tanstack/react-query";
import { Languages, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import { formatDuration, ipc, type SessionDetail } from "../../lib/ipc";
import { LANGUAGES, languageHint } from "../recorder/StartDialog";
import { useRetranscribe } from "./retranscribe";

/** Transcribing takes roughly a fifth of the meeting's length on an M-series Mac. */
export function estimate(durationMs: number) {
  const minutes = Math.max(1, Math.round(durationMs / 60_000 / 5));
  return `about ${minutes} minute${minutes === 1 ? "" : "s"}`;
}

export function RetranscribeDialog({
  session,
  onClose,
}: {
  session: SessionDetail;
  onClose: () => void;
}) {
  const run = useRetranscribe((s) => s.run);
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const recorded = useQuery({
    queryKey: ["session-languages", session.id],
    queryFn: () => ipc.sessionLanguages(session.id),
  });
  const [languages, setLanguages] = useState<string[] | null>(null);
  const chosen = languages ?? recorded.data ?? settings.data?.languages ?? [];
  const toggle = (code: string) =>
    setLanguages(chosen.includes(code) ? chosen.filter((l) => l !== code) : [...chosen, code]);

  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center p-6">
      <button
        type="button"
        aria-label="Cancel"
        className="absolute inset-0 cursor-default bg-ink/25 backdrop-blur-[2px]"
        onClick={onClose}
      />
      <form
        className="relative flex w-[480px] flex-col gap-5 rounded-2xl bg-surface p-7 shadow-2xl"
        onSubmit={(e) => {
          e.preventDefault();
          run(session.id, chosen);
          onClose();
        }}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <div className="flex items-start justify-between">
          <div>
            <p className="eyebrow flex items-center gap-1.5 text-accent">
              <RotateCcw className="size-3.5" /> Re-transcribe
            </p>
            <h2 className="mt-1 font-serif text-[26px] leading-8">Transcribe this session again</h2>
          </div>
          <button type="button" className="btn-quiet -mr-2" aria-label="Close" onClick={onClose}>
            <X className="size-4" />
          </button>
        </div>
        <p className="text-[14px] text-ink-soft">
          Runs the saved audio ({formatDuration(session.duration_ms)}) through the current
          transcription and speaker detection. The transcript and speaker labels are replaced; your
          notes stay as they are. The first transcript is kept beside the audio.
        </p>
        <section className="rounded-xl bg-sidebar p-4">
          <h3 className="flex items-center gap-2 font-serif text-lg">
            <Languages className="size-4 text-accent" /> Languages spoken
          </h3>
          <div className="mt-3 flex flex-wrap gap-2">
            {LANGUAGES.map(([code, name]) => (
              <button
                key={code}
                type="button"
                aria-pressed={chosen.includes(code)}
                onClick={() => toggle(code)}
                className={`rounded-full px-3 py-1 text-[13px] font-medium ${
                  chosen.includes(code)
                    ? "bg-accent text-on-accent"
                    : "bg-surface text-ink hover:bg-high"
                }`}
              >
                {name}
              </button>
            ))}
          </div>
          <p className="mt-2 text-[12px] text-ink-soft">{languageHint(chosen.length)}</p>
        </section>
        <div className="flex items-center justify-between">
          <span className="text-[12px] text-ink-soft">Takes {estimate(session.duration_ms)}.</span>
          <div className="flex gap-2">
            <button type="button" className="btn-quiet" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="btn-primary">
              <RotateCcw className="size-3.5" /> Re-transcribe
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
