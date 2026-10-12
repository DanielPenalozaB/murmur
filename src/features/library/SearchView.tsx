import { useQuery } from "@tanstack/react-query";
import { formatDate, ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { formatTimestamp } from "../transcript/group";

/** Renders FTS snippets, where matches come wrapped in [brackets]. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]*\])/g);
  return (
    <p className="leading-relaxed">
      {parts.map((part, i) =>
        part.startsWith("[") && part.endsWith("]") ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are static for a given snippet
          <mark key={i} className="rounded bg-accent/15 px-0.5 text-ink">
            {part.slice(1, -1)}
          </mark>
        ) : (
          part
        ),
      )}
    </p>
  );
}

export function SearchView({ query }: { query: string }) {
  const go = useNav((s) => s.go);
  const hits = useQuery({ queryKey: ["search", query], queryFn: () => ipc.search(query) });

  return (
    <div className="flex-1 overflow-y-auto px-8 pb-8 pt-12">
      <div className="mx-auto max-w-3xl">
        <p className="text-xs uppercase tracking-wide text-ink-soft">Search</p>
        <h2 className="mb-6 font-serif text-3xl">“{query}”</h2>
        {hits.data?.length === 0 && <p className="text-ink-soft">No transcript mentions that.</p>}
        <div className="flex flex-col gap-3">
          {hits.data?.map((h) => (
            <button
              key={`${h.session_id}-${h.start}`}
              type="button"
              onClick={() => go({ kind: "session", id: h.session_id })}
              className="rounded-xl border border-line bg-surface p-4 text-left hover:border-accent/40"
            >
              <div className="mb-1 flex items-center gap-2 text-xs text-ink-soft">
                <span className="font-medium text-ink">{h.title}</span>
                <span>·</span>
                <span>{formatDate(h.started_at)}</span>
                <span>·</span>
                <span className="tabular-nums">at {formatTimestamp(h.start)}</span>
              </div>
              <Snippet text={h.snippet} />
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
