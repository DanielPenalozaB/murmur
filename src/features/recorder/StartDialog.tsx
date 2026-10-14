import { useQuery } from "@tanstack/react-query";
import { Circle, Cloud, Folder, Languages, Mic, Radio, Sparkles, Speaker, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { type ExportFormat, ipc, type NewSession } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { flattenFolders } from "../library/folders";

export const LANGUAGES = [
  ["en", "English"],
  ["es", "Spanish"],
  ["pt", "Portuguese"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
] as const;

export function languageHint(count: number) {
  if (count === 0) return "Detected automatically for each sentence.";
  if (count === 1) return "Everything is transcribed in this language.";
  return "Murmur follows switches between these languages.";
}

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
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  const go = useNav((s) => s.go);
  const [title, setTitle] = useState(defaultTitle);
  const [folder, setFolder] = useState<string | null>(null);
  const [format, setFormat] = useState<ExportFormat | null>(null);
  const [languages, setLanguages] = useState<string[] | null>(null);
  const titleInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    titleInput.current?.select();
  }, []);

  const chosenFolder = folder ?? settings.data?.last_folder ?? "";
  const chosenFormat = format ?? settings.data?.default_format ?? "markdown";
  const chosenLanguages = languages ?? settings.data?.languages ?? [];
  const options = flattenFolders(folders.data?.tree ?? []);
  const toggleLanguage = (code: string) =>
    setLanguages(
      chosenLanguages.includes(code)
        ? chosenLanguages.filter((l) => l !== code)
        : [...chosenLanguages, code],
    );
  const local = info.data?.local ?? true;

  return (
    <div className="fixed inset-0 z-20 flex items-center justify-center p-6">
      <button
        type="button"
        aria-label="Cancel"
        className="absolute inset-0 cursor-default bg-ink/25 backdrop-blur-[2px]"
        onClick={onCancel}
      />
      <form
        className="relative flex max-h-full w-[520px] flex-col overflow-hidden rounded-2xl bg-surface shadow-2xl"
        onSubmit={(e) => {
          e.preventDefault();
          onStart({
            title: title.trim() || "Meeting",
            folder: chosenFolder,
            format: chosenFormat,
            languages: chosenLanguages,
          });
        }}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
      >
        <header className="bg-muted px-7 pb-6 pt-6">
          <div className="flex items-start justify-between">
            <p className="eyebrow flex items-center gap-1.5 text-accent">
              <Sparkles className="size-3.5" /> Pre-flight
            </p>
            <button
              type="button"
              className="btn-quiet -mr-2 -mt-1"
              aria-label="Close"
              onClick={onCancel}
            >
              <X className="size-4" />
            </button>
          </div>
          <h2 className="mt-1 font-serif text-[28px]">New Meeting Session</h2>
          <p className="mt-1 text-[14px] text-ink-soft">
            Name the session, choose where its notes go, and tell Murmur which languages to expect.
          </p>
          <div className="mt-5 grid grid-cols-[1fr_170px] gap-3">
            <label className="flex flex-col gap-1.5">
              <span className="eyebrow">Session title</span>
              <input
                ref={titleInput}
                className="rounded-lg bg-surface px-3 py-2 font-serif text-lg outline-none focus:ring-1 focus:ring-accent/40"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="eyebrow">Notebook</span>
              <span className="flex items-center gap-2 rounded-lg bg-surface px-3 py-2.5">
                <Folder className="size-4 shrink-0 text-accent" />
                <select
                  className="min-w-0 flex-1 bg-transparent text-[14px] outline-none"
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
              </span>
            </label>
          </div>
        </header>

        <div className="flex flex-col gap-6 overflow-y-auto px-7 py-6">
          <section className="rounded-xl bg-sidebar p-4">
            <h3 className="flex items-center gap-2 font-serif text-lg">
              <Languages className="size-4 text-accent" /> Languages spoken
            </h3>
            <div className="mt-3 flex flex-wrap gap-2">
              {LANGUAGES.map(([code, name]) => (
                <button
                  key={code}
                  type="button"
                  aria-pressed={chosenLanguages.includes(code)}
                  onClick={() => toggleLanguage(code)}
                  className={`rounded-full px-3 py-1 text-[13px] font-medium ${
                    chosenLanguages.includes(code)
                      ? "bg-accent text-on-accent"
                      : "bg-surface text-ink hover:bg-high"
                  }`}
                >
                  {name}
                </button>
              ))}
            </div>
            <p className="mt-2 text-[12px] text-ink-soft">{languageHint(chosenLanguages.length)}</p>
          </section>

          <section>
            <h3 className="flex items-center gap-2 font-serif text-lg">
              <Radio className="size-4 text-accent" /> Audio
              <span className="eyebrow ml-auto flex items-center gap-1.5 font-sans text-local">
                <span className="size-1.5 rounded-full bg-local" /> Ready
              </span>
            </h3>
            <div className="mt-3 grid grid-cols-2 gap-3 text-[13px]">
              <div className="rounded-xl bg-muted p-3">
                <p className="eyebrow">Microphone (you)</p>
                <p className="mt-2 flex items-center gap-2 font-medium">
                  <Mic className="size-4 text-ink-soft" /> Default input
                </p>
              </div>
              <div className="rounded-xl bg-muted p-3">
                <p className="eyebrow">System audio</p>
                <p className="mt-2 flex items-center gap-2 font-medium">
                  <Speaker className="size-4 text-ink-soft" /> Teams, Meet, Slack…
                </p>
              </div>
            </div>
            <p className="mt-2 text-[12px] text-ink-soft">
              People on the call are told apart by voice when the meeting ends.
            </p>
          </section>

          <section>
            <h3 className="eyebrow">AI engine for notes and questions</h3>
            <div className="mt-2 grid grid-cols-2 gap-3 text-[13px]">
              {[
                {
                  isLocal: true,
                  name: "Local Ollama",
                  detail: "Runs on this Mac. Nothing leaves it.",
                },
                {
                  isLocal: false,
                  name: "Cloud API",
                  detail: "Claude or Gemini with your own key.",
                },
              ].map((o) => (
                <div
                  key={o.name}
                  className={`rounded-xl p-3 ${local === o.isLocal ? "bg-accent-soft/50 ring-1 ring-accent/30" : "bg-muted opacity-70"}`}
                >
                  <p className="flex items-center gap-2 font-semibold">
                    {o.isLocal ? <Circle className="size-3.5" /> : <Cloud className="size-3.5" />}
                    {o.name}
                    {o.isLocal && (
                      <span className="chip ml-auto bg-local-soft text-on-local-soft">
                        Air-gapped
                      </span>
                    )}
                  </p>
                  <p className="mt-1 text-ink-soft">
                    {local === o.isLocal && info.data ? info.data.llm : o.detail}
                  </p>
                </div>
              ))}
            </div>
            <button
              type="button"
              className="mt-2 text-[12px] text-accent hover:underline"
              onClick={() => {
                onCancel();
                go({ kind: "settings" });
              }}
            >
              Change in Settings
            </button>
          </section>

          <section className="flex items-center justify-between gap-3">
            <span className="eyebrow">Save as</span>
            <div className="flex gap-1 rounded-lg bg-muted p-1">
              {(["markdown", "txt"] as const).map((f) => (
                <button
                  key={f}
                  type="button"
                  onClick={() => setFormat(f)}
                  className={`rounded-md px-3 py-1 text-[13px] ${chosenFormat === f ? "bg-surface font-semibold shadow-sm" : "text-ink-soft"}`}
                >
                  {f === "markdown" ? "Markdown (.md)" : "Plain text (.txt)"}
                </button>
              ))}
            </div>
          </section>
        </div>

        <footer className="flex items-center justify-between bg-muted px-7 py-4">
          <button type="button" className="btn-quiet" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="btn-primary px-5 py-2.5">
            <Circle className="size-3.5 fill-current" /> Start recording
            <span className="ml-1 opacity-70">↵</span>
          </button>
        </footer>
      </form>
    </div>
  );
}
