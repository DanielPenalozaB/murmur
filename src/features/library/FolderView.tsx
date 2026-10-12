import { useQuery } from "@tanstack/react-query";
import { formatDate, formatDuration, ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";

export function FolderView({ folder }: { folder: string | null }) {
  const go = useNav((s) => s.go);
  const sessions = useQuery({
    queryKey: ["sessions", folder],
    queryFn: () => ipc.listSessions(folder),
  });
  const title =
    folder === null ? "All sessions" : folder === "" ? "Vault root" : folder.split("/").pop();

  return (
    <div className="flex-1 overflow-y-auto px-8 pb-8 pt-12">
      <div className="mx-auto max-w-3xl">
        {folder?.includes("/") && (
          <p className="text-xs uppercase tracking-wide text-ink-soft">
            {folder.replaceAll("/", " / ")}
          </p>
        )}
        <h2 className="mb-6 font-serif text-3xl">{title}</h2>
        {sessions.error && <p className="text-accent">{String(sessions.error)}</p>}
        {sessions.data?.length === 0 && (
          <p className="text-ink-soft">
            No sessions here yet. Recordings saved to this folder appear here.
          </p>
        )}
        <div className="flex flex-col gap-3">
          {sessions.data?.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => go({ kind: "session", id: s.id })}
              className="rounded-xl border border-line bg-surface p-4 text-left hover:border-accent/40"
            >
              <div className="flex items-center gap-2 text-xs text-ink-soft">
                {folder === null && s.folder && (
                  <span className="rounded bg-muted px-1.5 py-0.5 text-ink">{s.folder}</span>
                )}
                <span>{formatDate(s.started_at)}</span>
                <span>·</span>
                <span>{formatDuration(s.duration_ms)}</span>
                <span>·</span>
                <span>.{s.format === "markdown" ? "md" : "txt"}</span>
              </div>
              <p className="mt-1 font-serif text-xl">{s.title}</p>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
