import { useQuery } from "@tanstack/react-query";
import { formatDate, formatDuration, ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { TranscriptTurns } from "../transcript/TranscriptTurns";

export function SessionView({ id }: { id: string }) {
  const go = useNav((s) => s.go);
  const session = useQuery({ queryKey: ["session", id], queryFn: () => ipc.getSession(id) });
  const s = session.data;

  return (
    <div className="flex-1 overflow-y-auto px-8 pb-8 pt-12 select-text">
      <div className="mx-auto max-w-3xl">
        {session.error && <p className="text-accent">{String(session.error)}</p>}
        {s && (
          <>
            <button
              type="button"
              className="text-xs uppercase tracking-wide text-ink-soft hover:text-ink"
              onClick={() => go({ kind: "folder", folder: s.folder })}
            >
              {s.folder ? s.folder.replaceAll("/", " / ") : "Vault root"}
            </button>
            <h2 className="mt-1 font-serif text-3xl">{s.title}</h2>
            <p className="mt-1 text-sm text-ink-soft">
              {formatDate(s.started_at)} · {formatDuration(s.duration_ms)}
            </p>

            <div className="mt-4 flex flex-wrap items-center gap-2 rounded-xl border border-line bg-surface p-3 text-sm">
              <span className="truncate text-ink-soft" title={s.file_path}>
                {s.file_path.replace(/^\/Users\/[^/]+/, "~")}
              </span>
              <div className="ml-auto flex gap-2">
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={() => ipc.openSessionItem(id, "open_file")}
                >
                  Open file
                </button>
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={() => ipc.openSessionItem(id, "reveal_file")}
                >
                  Reveal in Finder
                </button>
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={() => ipc.openSessionItem(id, "reveal_audio")}
                >
                  Audio
                </button>
              </div>
            </div>

            <h3 className="mb-4 mt-8 text-xs uppercase tracking-wide text-ink-soft">Transcript</h3>
            {s.segments.length === 0 ? (
              <p className="text-ink-soft">No speech was transcribed in this session.</p>
            ) : (
              <TranscriptTurns segments={s.segments} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
