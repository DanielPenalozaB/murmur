import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { type ExportFormat, ipc, type NewSession } from "../../lib/ipc";
import { flattenFolders } from "../library/folders";

function defaultTitle() {
  const now = new Date();
  return `Meeting ${now.toLocaleDateString(undefined, { month: "short", day: "numeric" })}, ${now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
}

export function StartDialog({
  onStart,
  onCancel,
}: {
  onStart: (session: NewSession) => void;
  onCancel: () => void;
}) {
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const folders = useQuery({ queryKey: ["folders"], queryFn: ipc.listFolders });
  const [title, setTitle] = useState(defaultTitle);
  const [folder, setFolder] = useState<string | null>(null);
  const [format, setFormat] = useState<ExportFormat | null>(null);
  const titleInput = useRef<HTMLInputElement>(null);

  useEffect(() => titleInput.current?.select(), []);

  const chosenFolder = folder ?? settings.data?.last_folder ?? "";
  const chosenFormat = format ?? settings.data?.default_format ?? "markdown";
  const options = flattenFolders(folders.data?.tree ?? []);

  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center">
      <button
        type="button"
        aria-label="Cancel"
        className="absolute inset-0 cursor-default bg-black/30"
        onClick={onCancel}
      />
      <form
        className="relative flex w-[440px] flex-col gap-4 rounded-2xl border border-line bg-surface p-6 shadow-xl"
        onSubmit={(e) => {
          e.preventDefault();
          onStart({ title: title.trim() || "Meeting", folder: chosenFolder, format: chosenFormat });
        }}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
      >
        <h2 className="font-serif text-2xl">New session</h2>

        <label className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">Title</span>
          <input
            ref={titleInput}
            className="input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>

        <label className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">Folder</span>
          <select
            className="input"
            value={chosenFolder}
            onChange={(e) => setFolder(e.target.value)}
          >
            <option value="">Vault root</option>
            {options.map((path) => (
              <option key={path} value={path}>
                {path.replaceAll("/", " / ")}
              </option>
            ))}
          </select>
        </label>

        <fieldset className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">Save as</span>
          <div className="flex gap-2">
            {(["markdown", "txt"] as const).map((f) => (
              <button
                key={f}
                type="button"
                onClick={() => setFormat(f)}
                className={`flex-1 rounded-lg border px-3 py-1.5 ${
                  chosenFormat === f ? "border-accent bg-accent/10 text-accent" : "border-line"
                }`}
              >
                {f === "markdown" ? "Markdown (.md)" : "Plain text (.txt)"}
              </button>
            ))}
          </div>
        </fieldset>

        <div className="mt-2 flex justify-end gap-2">
          <button type="button" className="btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="btn-primary">
            Start recording
          </button>
        </div>
      </form>
    </div>
  );
}
