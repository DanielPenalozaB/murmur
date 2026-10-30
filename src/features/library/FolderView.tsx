import { useQuery } from "@tanstack/react-query";
import {
  ArrowRight,
  AudioLines,
  CheckCircle2,
  Clock,
  ExternalLink,
  FileText,
  Lock,
} from "lucide-react";
import { useState } from "react";
import { AvatarStack, PageHeader, TopBar } from "../../components/ui";
import { formatDate, formatDuration, formatRelative, ipc, type SessionCard } from "../../lib/ipc";
import { folderCrumbs, useNav } from "../../lib/nav";
import { Markdown } from "../assistant/Markdown";
import { actionItems, parseSections } from "../assistant/notes";
import { FolderActions } from "./manage";

function Card({
  card,
  selected,
  showFolder,
  onSelect,
  onOpen,
}: {
  card: SessionCard;
  selected: boolean;
  showFolder: boolean;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const people = ["Me", ...card.speakers];
  const total = card.actions_open + card.actions_done;
  return (
    <button
      type="button"
      onClick={onSelect}
      onDoubleClick={onOpen}
      className={`card flex flex-col gap-2.5 p-5 text-left transition-shadow hover:shadow-md ${
        selected ? "border-l-4 border-accent" : ""
      }`}
    >
      <div className="flex flex-wrap items-center gap-2 text-[13px] text-ink-soft">
        {showFolder && (
          <span className="chip bg-local-soft text-on-local-soft">
            {card.folder || "Vault root"}
          </span>
        )}
        <span>{formatRelative(card.started_at)}</span>
        <span className="text-outline">•</span>
        <span className="font-medium text-ink">
          {formatDuration(card.duration_ms)} · .{card.format === "markdown" ? "md" : "txt"}
        </span>
      </div>
      <h3 className="font-serif text-[22px] leading-7">{card.title}</h3>
      {card.excerpt && <p className="line-clamp-2 text-ink-soft">{card.excerpt}</p>}
      <div className="mt-1 flex items-center gap-3 text-[13px] text-ink-soft">
        <AvatarStack names={people} />
        <span className="truncate">{people.map((p) => (p === "Me" ? "You" : p)).join(", ")}</span>
        {total > 0 && (
          <span
            className={`chip ml-auto shrink-0 whitespace-nowrap ${card.actions_open === 0 ? "bg-local-soft text-on-local-soft" : "bg-muted text-ink"}`}
          >
            <CheckCircle2 className="size-3.5" />
            {card.actions_open === 0
              ? "All done"
              : `${card.actions_open} action item${card.actions_open === 1 ? "" : "s"}`}
          </span>
        )}
      </div>
    </button>
  );
}

function Preview({ id }: { id: string }) {
  const go = useNav((s) => s.go);
  const session = useQuery({ queryKey: ["session", id], queryFn: () => ipc.getSession(id) });
  const s = session.data;
  if (!s) return <div className="card h-64 animate-pulse" />;
  const summary = parseSections(s.notes).sections.find((x) => x.title.toLowerCase() === "summary");
  const items = actionItems(s.notes);

  return (
    <aside className="card flex min-h-0 flex-col overflow-hidden">
      <div className="flex items-center justify-between bg-sidebar px-5 py-3">
        <span className="flex items-center gap-2 text-[12px] font-medium text-ink-soft">
          <span className="size-2 rounded-full bg-local" /> Local document preview
        </span>
        <button
          type="button"
          className="btn-quiet"
          title="Open file"
          onClick={() => ipc.openSessionItem(id, "open_file")}
        >
          <ExternalLink className="size-4" />
        </button>
      </div>
      <div className="flex min-h-0 flex-col gap-5 overflow-y-auto p-5">
        <div>
          <span className="chip bg-local-soft text-on-local-soft">{s.folder || "Vault root"}</span>
          <h2 className="mt-2 font-serif text-[28px] leading-8">{s.title}</h2>
          <p className="mt-3 flex items-center gap-2 text-[13px] text-ink-soft">
            <Clock className="size-3.5" /> {formatDate(s.started_at)}
          </p>
          <p className="mt-1 flex items-center gap-2 text-[13px] text-ink-soft">
            <AudioLines className="size-3.5" /> {formatDuration(s.duration_ms)} transcript
          </p>
        </div>
        {summary ? (
          <div className="rounded-xl border-l-4 border-accent bg-sidebar p-4">
            <p className="mb-2 font-serif text-lg">Executive Synthesis</p>
            <div className="text-[14px]">
              <Markdown text={summary.body} />
            </div>
          </div>
        ) : (
          <p className="text-[14px] text-ink-soft">No notes written yet.</p>
        )}
        {items.length > 0 && (
          <div className="flex flex-col gap-2">
            <span className="eyebrow">Action items &amp; owners</span>
            {items.map((item) => (
              <div key={item.line} className="flex gap-2 text-[14px]">
                <input
                  type="checkbox"
                  checked={item.done}
                  disabled
                  className="mt-1 accent-accent"
                />
                <div className={item.done ? "text-ink-soft line-through" : ""}>
                  {item.text.replace(/\s*\[[\d:,\s–-]+\]/g, "")}
                  {item.owner && (
                    <p className="text-[12px] text-accent">Assigned to {item.owner}</p>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
        <button type="button" className="btn-ghost" onClick={() => go({ kind: "session", id })}>
          <FileText className="size-4" /> Open note
        </button>
      </div>
    </aside>
  );
}

export function FolderView({ folder }: { folder: string | null }) {
  const go = useNav((s) => s.go);
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const sessions = useQuery({
    queryKey: ["sessions", folder],
    queryFn: () => ipc.listSessions(folder),
  });
  const [selected, setSelected] = useState<string | null>(null);
  const cards = sessions.data ?? [];
  const current = selected && cards.some((c) => c.id === selected) ? selected : cards[0]?.id;
  const name =
    folder === null
      ? "All Notes & Transcripts"
      : folder === ""
        ? "Vault root"
        : (folder.split("/").pop() ?? "");
  const vault = settings.data?.vault_path.replace(/^\/Users\/[^/]+/, "~") ?? "";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar
        crumbs={[
          ...folderCrumbs(folder),
          ...(folder === "" ? ["Vault root"] : folder === null ? ["All notes"] : []),
        ]}
      />
      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        <div className="mx-auto flex max-w-[1180px] flex-col gap-8 pt-2">
          <PageHeader
            eyebrow={
              <>
                <span className="size-1.5 rounded-full bg-accent" />
                <span className="text-accent">Indexed local repository</span>
                <span className="text-outline">/</span>
                <span className="normal-case tracking-normal">
                  {vault}
                  {folder ? `/${folder}` : ""}
                </span>
              </>
            }
            title={name}
            subtitle={`${cards.length} ${cards.length === 1 ? "session" : "sessions"} stored on this Mac.`}
          >
            <div className="flex items-center gap-1">
              {folder && sessions.isSuccess && (
                <FolderActions folder={folder} sessions={cards.length} />
              )}
              <span className="chip ml-2 gap-1.5 bg-local-soft py-1 text-on-local-soft">
                <Lock className="size-3" /> 100% local
              </span>
            </div>
          </PageHeader>

          {sessions.error && <p className="text-accent">{String(sessions.error)}</p>}
          {cards.length === 0 && sessions.isSuccess && (
            <div className="card p-8 text-center text-ink-soft">
              No sessions here yet. Recordings saved to this notebook appear here.
            </div>
          )}

          {cards.length > 0 && (
            <div className="grid grid-cols-[minmax(0,1fr)_400px] items-start gap-6">
              {/* minmax(0, …): a card with a long speaker list must not widen the column. */}
              <div className="flex flex-col gap-4">
                <div className="flex items-center justify-between px-1">
                  <span className="eyebrow">Recent captures &amp; syntheses</span>
                  <span className="text-[12px] text-ink-soft">Sorted by recency</span>
                </div>
                {cards.map((c) => (
                  <Card
                    key={c.id}
                    card={c}
                    selected={c.id === current}
                    showFolder={folder === null}
                    onSelect={() => setSelected(c.id)}
                    onOpen={() => go({ kind: "session", id: c.id })}
                  />
                ))}
                <p className="flex items-center gap-1 px-1 text-[12px] text-ink-soft">
                  Double-click a note to open it <ArrowRight className="size-3" />
                </p>
              </div>
              {/* Fits the scroll area (window minus the 3.5rem top bar and 2rem bottom padding), so a
                  long preview scrolls inside the panel instead of with the list. */}
              <div className="sticky top-0 flex max-h-[calc(100vh-5.5rem)] flex-col">
                {current && <Preview id={current} />}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
