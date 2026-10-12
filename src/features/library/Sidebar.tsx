import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { type FolderNode, ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";

function Item({
  label,
  count,
  active,
  depth = 0,
  onClick,
  dot,
}: {
  label: string;
  count?: number;
  active: boolean;
  depth?: number;
  onClick: () => void;
  dot?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{ paddingLeft: 8 + depth * 14 }}
      className={`flex w-full items-center gap-2 rounded-md py-1.5 pr-2 text-left text-sm ${
        active ? "bg-muted font-medium" : "hover:bg-muted/60"
      }`}
    >
      {dot && <span className="size-2 animate-pulse rounded-full bg-accent" />}
      <span className="flex-1 truncate">{label}</span>
      {count !== undefined && count > 0 && <span className="text-xs text-ink-soft">{count}</span>}
    </button>
  );
}

export function Sidebar({ recording }: { recording: boolean }) {
  const { view, go } = useNav();
  const queryClient = useQueryClient();
  const folders = useQuery({ queryKey: ["folders"], queryFn: ipc.listFolders });
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const [query, setQuery] = useState("");
  const [newFolder, setNewFolder] = useState<string | null>(null);

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
    <aside className="flex min-h-0 flex-col border-r border-line bg-sidebar px-3 pt-11">
      <div className="h-6" data-tauri-drag-region />
      <h1 className="px-2 font-serif text-xl">Murmur</h1>

      <form
        className="mt-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (query.trim()) go({ kind: "search", query: query.trim() });
        }}
      >
        <input
          className="input w-full"
          placeholder="Search transcripts"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </form>

      <nav className="mt-4 flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        <Item
          label={recording ? "Recording now" : "Live"}
          active={view.kind === "live"}
          dot={recording}
          onClick={() => go({ kind: "live" })}
        />
        <Item
          label="All sessions"
          count={total}
          active={isFolder(null)}
          onClick={() => go({ kind: "folder", folder: null })}
        />

        <div className="mt-4 flex items-center justify-between px-2">
          <span className="text-xs uppercase tracking-wide text-ink-soft">Folders</span>
          <button
            type="button"
            className="text-lg leading-none text-ink-soft hover:text-ink"
            title="New folder"
            onClick={() => setNewFolder("")}
          >
            +
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
            label="Vault root"
            count={counts[""]}
            active={isFolder("")}
            onClick={() => go({ kind: "folder", folder: "" })}
          />
        ) : null}
        {renderNodes(folders.data?.tree ?? [], 0)}
        {folders.data?.tree.length === 0 && (
          <p className="px-2 py-1 text-xs text-ink-soft">No folders yet. Click + to add one.</p>
        )}
      </nav>

      {settings.data && (
        <p className="truncate py-3 px-2 text-xs text-ink-soft" title={settings.data.vault_path}>
          Vault: {settings.data.vault_path.replace(/^\/Users\/[^/]+/, "~")}
        </p>
      )}
    </aside>
  );
}
