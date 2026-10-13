import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { useState } from "react";
import { Avatar, PageHeader, TopBar } from "../../components/ui";
import { formatRelative, ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { formatTimestamp } from "../transcript/group";

/** Renders FTS snippets, where matches come wrapped in [brackets]. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]*\])/g);
  return (
    <blockquote className="border-l-2 border-line pl-3 italic leading-relaxed text-ink-soft">
      “
      {parts.map((part, i) =>
        part.startsWith("[") && part.endsWith("]") ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are static for a given snippet
          <mark key={i} className="rounded bg-accent-soft px-0.5 not-italic text-on-accent-soft">
            {part.slice(1, -1)}
          </mark>
        ) : (
          part
        ),
      )}
      ”
    </blockquote>
  );
}

function SpeakerDirectoryCard() {
  const go = useNav((s) => s.go);
  const voices = useQuery({ queryKey: ["voices"], queryFn: ipc.listVoices });
  return (
    <aside className="card flex flex-col gap-4 p-5">
      <div>
        <h3 className="font-serif text-xl">Speaker Directory</h3>
        <p className="text-[13px] text-ink-soft">Voiceprints &amp; meeting frequency</p>
      </div>
      {voices.data?.length === 0 && (
        <p className="text-[13px] text-ink-soft">
          Name a speaker in any meeting and Murmur will recognize them next time.
        </p>
      )}
      {voices.data?.slice(0, 6).map((v) => (
        <button
          key={v.name}
          type="button"
          className="flex items-center gap-3 rounded-lg p-1.5 text-left hover:bg-muted"
          onClick={() => go({ kind: "speakers" })}
        >
          <Avatar name={v.name} size="lg" />
          <span className="min-w-0">
            <span className="block truncate font-medium">{v.name}</span>
            <span className="text-[13px] text-ink-soft">
              {v.meetings} {v.meetings === 1 ? "meeting" : "meetings"}
            </span>
          </span>
        </button>
      ))}
    </aside>
  );
}

export function SearchView({ query }: { query: string }) {
  const go = useNav((s) => s.go);
  const [text, setText] = useState(query);
  const hits = useQuery({ queryKey: ["search", query], queryFn: () => ipc.search(query) });
  const count = hits.data?.length ?? 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={["Vault", "Search"]} />
      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        <div className="mx-auto flex max-w-[1180px] flex-col gap-6 pt-2">
          <PageHeader
            eyebrow={
              <>
                <span>Vault</span>
                <span className="text-outline">/</span>
                <span className="text-accent">Corpus search</span>
              </>
            }
            title="Notes & Corpus"
            subtitle={`${count} ${count === 1 ? "line matches" : "lines match"} “${query}” across your transcripts.`}
          />
          <form
            className="card flex items-center gap-3 px-4 py-3"
            onSubmit={(e) => {
              e.preventDefault();
              if (text.trim()) go({ kind: "search", query: text.trim() });
            }}
          >
            <Search className="size-4 text-ink-soft" />
            <input
              className="flex-1 bg-transparent outline-none placeholder:text-outline"
              placeholder="Search transcripts…"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <span className="kbd">⌘K</span>
          </form>

          <div className="grid grid-cols-[1fr_340px] items-start gap-6">
            <div className="flex flex-col gap-4">
              {hits.data?.length === 0 && (
                <div className="card p-8 text-center text-ink-soft">
                  No transcript mentions that.
                </div>
              )}
              {hits.data?.map((h) => (
                <button
                  key={`${h.session_id}-${h.start}`}
                  type="button"
                  onClick={() => go({ kind: "session", id: h.session_id })}
                  className="card flex flex-col gap-3 p-5 text-left transition-shadow hover:shadow-md"
                >
                  <div className="flex items-center gap-2 text-[13px] text-ink-soft">
                    <span className="chip bg-local-soft text-on-local-soft">
                      {h.folder || "Vault root"}
                    </span>
                    <span>{formatRelative(h.started_at)}</span>
                  </div>
                  <h3 className="font-serif text-[22px] leading-7">{h.title}</h3>
                  <div className="flex items-center gap-2 text-[13px]">
                    <Avatar name={h.speaker} size="xs" />
                    <span className="font-semibold">{h.speaker === "Me" ? "You" : h.speaker}</span>
                    <span className="chip bg-muted tabular-nums text-ink-soft">
                      {formatTimestamp(h.start)}
                    </span>
                  </div>
                  <Snippet text={h.snippet} />
                </button>
              ))}
            </div>
            <SpeakerDirectoryCard />
          </div>
        </div>
      </div>
    </div>
  );
}
