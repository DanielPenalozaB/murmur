import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AudioLines,
  Ellipsis,
  FolderInput,
  type LucideIcon,
  PencilLine,
  Trash2,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { formatBytes, ipc, type SessionDetail } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { flattenFolders } from "./folders";

/** `Work/1-1s` renamed to `Team` → `Work/Team`. */
export function renamedPath(path: string, name: string) {
  const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/") + 1) : "";
  return parent + name.trim();
}

/** Lists, folders and open sessions all show what changed. */
function useRefresh() {
  const queryClient = useQueryClient();
  return () =>
    Promise.all(
      [["sessions"], ["folders"], ["session"], ["storage"], ["tasks"]].map((queryKey) =>
        queryClient.invalidateQueries({ queryKey }),
      ),
    );
}

export function Dialog({
  icon: Icon,
  eyebrow,
  title,
  children,
  submit,
  danger,
  onSubmit,
  onClose,
}: {
  icon: LucideIcon;
  eyebrow: string;
  title: string;
  children: ReactNode;
  submit: string;
  danger?: boolean;
  onSubmit: () => Promise<void>;
  onClose: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center p-6">
      <button
        type="button"
        aria-label="Cancel"
        className="absolute inset-0 cursor-default bg-ink/25 backdrop-blur-[2px]"
        onClick={onClose}
      />
      <form
        className="relative flex w-[440px] flex-col gap-5 rounded-2xl bg-surface p-7 shadow-2xl"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            await onSubmit();
            onClose();
          } catch (err) {
            setError(String(err));
          } finally {
            setBusy(false);
          }
        }}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <div className="flex items-start justify-between">
          <div>
            <p className={`eyebrow flex items-center gap-1.5 ${danger ? "text-accent" : ""}`}>
              <Icon className="size-3.5" /> {eyebrow}
            </p>
            <h2 className="mt-1 font-serif text-[26px] leading-8">{title}</h2>
          </div>
          <button type="button" className="btn-quiet -mr-2" aria-label="Close" onClick={onClose}>
            <X className="size-4" />
          </button>
        </div>
        {children}
        {error && <p className="text-[13px] text-accent">{error}</p>}
        <div className="flex justify-end gap-2">
          <button
            type="button"
            className="btn-ghost"
            // biome-ignore lint/a11y/noAutofocus: so Enter can't delete by accident
            autoFocus={danger}
            onClick={onClose}
          >
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={busy}>
            {submit}
          </button>
        </div>
      </form>
    </div>
  );
}

type SessionAction = "rename" | "move" | "delete-audio" | "delete";

/** The session page's "…" menu: rename, move, and delete the audio or the session. */
export function SessionMenu({ session, busy }: { session: SessionDetail; busy: boolean }) {
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState<SessionAction | null>(null);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  const items: [SessionAction, LucideIcon, string, boolean][] = [
    ["rename", PencilLine, "Rename", false],
    ["move", FolderInput, "Move to folder", false],
    [
      "delete-audio",
      AudioLines,
      session.audio_bytes > 0
        ? `Delete audio (${formatBytes(session.audio_bytes)})`
        : "Audio already deleted",
      session.audio_bytes === 0,
    ],
    ["delete", Trash2, "Delete session", false],
  ];

  return (
    <div ref={menu} className="relative">
      <button
        type="button"
        className="btn-quiet"
        title="More"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen(!open)}
      >
        <Ellipsis className="size-4" />
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full z-10 mt-1 flex w-56 flex-col rounded-xl bg-surface p-1 shadow-xl ring-1 ring-line"
        >
          {items.map(([id, Icon, label, disabled]) => (
            <button
              key={id}
              type="button"
              role="menuitem"
              disabled={disabled}
              className={`flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] hover:bg-muted disabled:opacity-50 disabled:hover:bg-transparent ${
                id === "delete" ? "text-accent" : ""
              }`}
              onClick={() => {
                setOpen(false);
                setAction(id);
              }}
            >
              <Icon className="size-4" /> {label}
            </button>
          ))}
        </div>
      )}
      {action && (
        <SessionDialog session={session} action={action} onClose={() => setAction(null)} />
      )}
    </div>
  );
}

function SessionDialog({
  session,
  action,
  onClose,
}: {
  session: SessionDetail;
  action: SessionAction;
  onClose: () => void;
}) {
  const go = useNav((s) => s.go);
  const refresh = useRefresh();
  const folders = useQuery({ queryKey: ["folders"], queryFn: ipc.listFolders });
  const [title, setTitle] = useState(session.title);
  const [folder, setFolder] = useState(session.folder);

  switch (action) {
    case "rename":
      return (
        <Dialog
          icon={PencilLine}
          eyebrow="Rename"
          title="Rename this session"
          submit="Rename"
          onClose={onClose}
          onSubmit={async () => {
            await ipc.renameSession(session.id, title);
            await refresh();
          }}
        >
          <input
            autoFocus
            className="input"
            aria-label="Title"
            value={title}
            maxLength={200}
            onChange={(e) => setTitle(e.target.value)}
          />
          <p className="-mt-2 text-[13px] text-ink-soft">The file in your vault is renamed too.</p>
        </Dialog>
      );
    case "move":
      return (
        <Dialog
          icon={FolderInput}
          eyebrow="Move"
          title="Move to folder"
          submit="Move"
          onClose={onClose}
          onSubmit={async () => {
            await ipc.moveSession(session.id, folder);
            await refresh();
          }}
        >
          <select
            className="input"
            aria-label="Folder"
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
          >
            <option value="">Vault root</option>
            {flattenFolders(folders.data?.tree ?? []).map((path) => (
              <option key={path} value={path}>
                {path}
              </option>
            ))}
          </select>
        </Dialog>
      );
    case "delete-audio":
      return (
        <Dialog
          icon={AudioLines}
          eyebrow="Delete audio"
          title="Delete the recordings?"
          submit={`Delete ${formatBytes(session.audio_bytes)}`}
          danger
          onClose={onClose}
          onSubmit={async () => {
            await ipc.deleteSessionAudio(session.id);
            await refresh();
          }}
        >
          <p className="text-[14px] text-ink-soft">
            Frees {formatBytes(session.audio_bytes)}. The transcript and notes stay, but this
            session can't be re-transcribed afterwards.
          </p>
        </Dialog>
      );
    case "delete":
      return (
        <Dialog
          icon={Trash2}
          eyebrow="Delete"
          title={`Delete “${session.title}”?`}
          submit="Delete session"
          danger
          onClose={onClose}
          onSubmit={async () => {
            await ipc.deleteSession(session.id);
            go({ kind: "folder", folder: session.folder });
            await refresh();
          }}
        >
          <p className="text-[14px] text-ink-soft">
            Deletes the file in your vault, the recordings and the notes. Remembered voices stay.
            This can't be undone.
          </p>
        </Dialog>
      );
  }
}

/** Rename and Delete for a folder's page. Only empty folders can be deleted. */
export function FolderActions({ folder, sessions }: { folder: string; sessions: number }) {
  const go = useNav((s) => s.go);
  const refresh = useRefresh();
  const [action, setAction] = useState<"rename" | "delete" | null>(null);
  const [name, setName] = useState("");
  const current = folder.split("/").pop() ?? folder;

  return (
    <>
      <button
        type="button"
        className="btn-quiet"
        title="Rename folder"
        onClick={() => {
          setName(current);
          setAction("rename");
        }}
      >
        <PencilLine className="size-4" />
      </button>
      <button
        type="button"
        className="btn-quiet"
        title={sessions > 0 ? "Move or delete its sessions first" : "Delete folder"}
        disabled={sessions > 0}
        onClick={() => setAction("delete")}
      >
        <Trash2 className="size-4" />
      </button>
      {action === "rename" && (
        <Dialog
          icon={PencilLine}
          eyebrow="Rename"
          title="Rename folder"
          submit="Rename"
          onClose={() => setAction(null)}
          onSubmit={async () => {
            if (name.includes("/")) throw new Error("A folder name can't contain /");
            const to = renamedPath(folder, name);
            if (to === folder) return;
            await ipc.moveFolder(folder, to);
            go({ kind: "folder", folder: to });
            await refresh();
          }}
        >
          <input
            autoFocus
            className="input"
            aria-label="Folder name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <p className="-mt-2 text-[13px] text-ink-soft">
            Its sessions and subfolders move with it.
          </p>
        </Dialog>
      )}
      {action === "delete" && (
        <Dialog
          icon={Trash2}
          eyebrow="Delete"
          title={`Delete “${current}”?`}
          submit="Delete folder"
          danger
          onClose={() => setAction(null)}
          onSubmit={async () => {
            await ipc.deleteFolder(folder);
            go({ kind: "folder", folder: null });
            await refresh();
          }}
        >
          <p className="text-[14px] text-ink-soft">
            The folder is empty. Folders with your own files in them are kept.
          </p>
        </Dialog>
      )}
    </>
  );
}
