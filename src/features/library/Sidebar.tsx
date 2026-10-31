import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Folder,
  Inbox,
  ListChecks,
  Mic,
  Plus,
  Search,
  SlidersHorizontal,
  SquarePen,
  UsersRound,
} from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { LogoMark } from "../../components/ui";
import { type FolderNode, ipc } from "../../lib/ipc";
import { useNav, useUi } from "../../lib/nav";
import { useTasks } from "../actions/ActionsView";
import { groupOf } from "../actions/actions";
import { useSpeakerReview } from "../speakers/SpeakerReview";
import { UpdateCard } from "../updates/UpdateCard";

function Item({
  icon,
  label,
  count,
  active,
  depth = 0,
  onClick,
  live,
  alert,
}: {
  icon: ReactNode;
  label: string;
  count?: number;
  /** Things that need the user's attention, shown as a badge. */
  alert?: { count: number; title: string };
  active: boolean;
  depth?: number;
  onClick: () => void;
  live?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{ paddingLeft: 10 + depth * 14 }}
      className={`flex w-full items-center gap-2.5 rounded-lg py-1.5 pr-2.5 text-left text-[14px] ${
        active ? "bg-high font-medium text-ink" : "text-ink-soft hover:bg-muted hover:text-ink"
      }`}
    >
      <span className="text-ink-soft [&>svg]:size-4">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {live && <span className="size-2 animate-pulse rounded-full bg-accent" />}
      {alert && alert.count > 0 && (
        <span
          title={alert.title}
          className="rounded-full bg-accent-soft px-1.5 text-[11px] font-semibold tabular-nums text-on-accent-soft"
        >
          {alert.count}
        </span>
      )}
      {count !== undefined && count > 0 && (
        <span className="text-xs tabular-nums text-outline">{count}</span>
      )}
    </button>
  );
}

export function Sidebar({ recording }: { recording: boolean }) {
  const myOpen = useTasks().tasks.filter((t) => t.mine && !t.done);
  const { view, go } = useNav();
  const setStartOpen = useUi((s) => s.setStartOpen);
  const queryClient = useQueryClient();
  const folders = useQuery({ queryKey: ["folders"], queryFn: ipc.listFolders });
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  const review = useSpeakerReview();
  const [query, setQuery] = useState("");
  const [newFolder, setNewFolder] = useState<string | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);

  // ⌘K searches from anywhere, as in the design.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "k") {
        e.preventDefault();
        searchInput.current?.focus();
        searchInput.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const create = useMutation({
    mutationFn: (path: string) => ipc.createFolder(path),
    onSuccess: (_, path) => {
      setNewFolder(null);
      queryClient.invalidateQueries({ queryKey: ["folders"] });
      go({ kind: "folder", folder: path });
    },
  });

  const counts = folders.data?.counts ?? {};
  const total = Object.values(counts).reduce((a, b) => a + b, 0);
  const isFolder = (f: string | null) => view.kind === "folder" && view.folder === f;

  const renderNodes = (nodes: FolderNode[], depth: number) =>
    nodes.map((node) => (
      <div key={node.path}>
        <Item
          icon={<Folder />}
          label={node.name}
          count={counts[node.path]}
          depth={depth}
          active={isFolder(node.path)}
          onClick={() => go({ kind: "folder", folder: node.path })}
        />
        {renderNodes(node.children, depth + 1)}
      </div>
    ));

  return (
    <aside className="flex min-h-0 flex-col bg-sidebar px-3 pt-10">
      <div className="h-2" data-tauri-drag-region />
      <div className="flex items-center gap-2.5 px-1.5" data-tauri-drag-region>
        <LogoMark />
        <span className="flex-1 font-serif text-xl">Murmur</span>
        <button
          type="button"
          className="rounded-lg bg-muted p-1.5 text-ink-soft hover:bg-high hover:text-ink disabled:opacity-40"
          title="New session"
          disabled={recording}
          onClick={() => {
            go({ kind: "live" });
            setStartOpen(true);
          }}
        >
          <SquarePen className="size-4" />
        </button>
      </div>

      <form
        className="mt-4 flex items-center gap-2 rounded-lg bg-muted px-2.5 py-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          if (query.trim()) go({ kind: "search", query: query.trim() });
        }}
      >
        <Search className="size-4 text-ink-soft" />
        <input
          ref={searchInput}
          className="min-w-0 flex-1 bg-transparent text-[14px] outline-none placeholder:text-ink-soft"
          placeholder="Search corpus"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="kbd">⌘K</span>
      </form>

      <nav className="mt-5 flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        <Item
          icon={<Mic />}
          label={recording ? "Recording now" : "Live"}
          active={view.kind === "live"}
          live={recording}
          onClick={() => go({ kind: "live" })}
        />
        <Item
          icon={<Inbox />}
          label="All notes"
          count={total}
          active={isFolder(null)}
          onClick={() => go({ kind: "folder", folder: null })}
        />

        <div className="mt-5 mb-1 flex items-center justify-between px-2.5">
          <span className="eyebrow">Notebooks</span>
          <button
            type="button"
            className="text-ink-soft hover:text-ink"
            title="New folder"
            onClick={() => setNewFolder("")}
          >
            <Plus className="size-4" />
          </button>
        </div>
        {newFolder !== null && (
          <form
            className="px-1 py-1"
            onSubmit={(e) => {
              e.preventDefault();
              if (newFolder.trim()) create.mutate(newFolder.trim());
            }}
          >
            <input
              // biome-ignore lint/a11y/noAutofocus: the input appears in response to the + click
              autoFocus
              className="input w-full"
              placeholder="Folder name, e.g. Work/1-1s"
              value={newFolder}
              onChange={(e) => setNewFolder(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setNewFolder(null)}
              onBlur={() => !newFolder.trim() && setNewFolder(null)}
            />
            {create.error && <p className="mt-1 text-xs text-accent">{String(create.error)}</p>}
          </form>
        )}
        {counts[""] ? (
          <Item
            icon={<Folder />}
            label="Vault root"
            count={counts[""]}
            active={isFolder("")}
            onClick={() => go({ kind: "folder", folder: "" })}
          />
        ) : null}
        {renderNodes(folders.data?.tree ?? [], 0)}
        {folders.data?.tree.length === 0 && (
          <p className="px-2.5 py-1 text-xs text-ink-soft">No folders yet. Click + to add one.</p>
        )}

        <span className="eyebrow mt-5 mb-1 px-2.5">Intelligence</span>
        <Item
          icon={<ListChecks />}
          label="Action items"
          count={myOpen.length}
          alert={{
            count: myOpen.filter((t) => groupOf(t, new Date()) === "overdue").length,
            title: "Your action items past their due date",
          }}
          active={view.kind === "actions"}
          onClick={() => go({ kind: "actions" })}
        />
        <Item
          icon={<UsersRound />}
          label="Speakers"
          alert={{
            count: review.data?.length ?? 0,
            title: "Speaker names to double-check",
          }}
          active={view.kind === "speakers"}
          onClick={() => go({ kind: "speakers" })}
        />
        <Item
          icon={<SlidersHorizontal />}
          label="Settings"
          active={view.kind === "settings"}
          onClick={() => go({ kind: "settings" })}
        />
      </nav>

      <UpdateCard recording={recording} />
      <div className="mb-2 flex flex-col gap-2 rounded-xl bg-muted p-3 text-xs">
        <div className="flex items-center justify-between">
          <span className="eyebrow">Indexed vault</span>
          <span className="text-ink-soft">
            {total} {total === 1 ? "note" : "notes"}
          </span>
        </div>
        <div className="h-1 overflow-hidden rounded-full bg-high">
          <div className="h-full w-full rounded-full bg-local" />
        </div>
        {info.data && (
          <div className="flex items-center justify-between gap-2">
            <span className="text-ink-soft">AI</span>
            <span className={`truncate ${info.data.local ? "text-local" : "text-accent"}`}>
              {info.data.llm}
            </span>
          </div>
        )}
      </div>
      <div className="flex items-center gap-2 px-1.5 pb-3 text-xs text-ink-soft">
        <span
          className={`size-2 rounded-full ${recording ? "animate-pulse bg-accent" : "bg-local"}`}
        />
        <span className="flex-1">{recording ? "Recording" : "Engine idle"}</span>
        {info.data && <span>v{info.data.version}</span>}
      </div>
    </aside>
  );
}
