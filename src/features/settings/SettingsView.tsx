import { useQuery, useQueryClient } from "@tanstack/react-query";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  AudioLines,
  BellOff,
  Cloud,
  Cpu,
  Download,
  FileText,
  FolderOpen,
  KeyRound,
  Languages,
  Radio,
  ShieldCheck,
  SpellCheck,
  Type,
  UserRound,
  UsersRound,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { PageHeader, Toggle, TopBar } from "../../components/ui";
import { formatBytes, ipc, type Provider, type Settings } from "../../lib/ipc";
import { folderCrumbs } from "../../lib/nav";
import { languageHint } from "../recorder/StartDialog";
import { useRecorder } from "../recorder/useRecorder";
import { installLabel, useInstall } from "../updates/updates";
import { parseVocabulary } from "./vocabulary";

type Kind = Provider["kind"];

export const DEFAULTS: Record<Kind, Provider> = {
  ollama: {
    kind: "ollama",
    base_url: "http://127.0.0.1:11434",
    model: "qwen3:14b",
    num_ctx: 32768,
  },
  anthropic: { kind: "anthropic", model: "claude-opus-5-5" },
  gemini: { kind: "gemini", model: "" },
};

const PROVIDERS: { kind: Kind; name: string; detail: string; icon: ReactNode }[] = [
  {
    kind: "ollama",
    name: "Local Ollama",
    detail: "Runs on this Mac. Nothing leaves it.",
    icon: <Cpu className="size-4" />,
  },
  {
    kind: "anthropic",
    name: "Claude",
    detail: "Anthropic API with your own key.",
    icon: <Cloud className="size-4" />,
  },
  {
    kind: "gemini",
    name: "Gemini",
    detail: "Google AI API with your own key.",
    icon: <Cloud className="size-4" />,
  },
];

const LANGUAGES = [
  ["en", "English"],
  ["es", "Spanish"],
  ["pt", "Portuguese"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
] as const;

function Stage({
  n,
  title,
  description,
  badge,
  children,
}: {
  n: number;
  title: string;
  description: string;
  badge?: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="flex items-baseline gap-2">
            <span className="eyebrow text-accent">Stage {String(n).padStart(2, "0")}</span>
            <span className="text-outline">•</span>
            <span className="font-serif text-2xl">{title}</span>
          </p>
          <p className="mt-1 text-[14px] text-ink-soft">{description}</p>
        </div>
        {badge && (
          <span className="chip shrink-0 bg-local-soft py-1 text-on-local-soft">{badge}</span>
        )}
      </div>
      <div className="card flex flex-col gap-5 p-6">{children}</div>
    </section>
  );
}

function Row({
  icon,
  title,
  detail,
  children,
}: {
  icon: ReactNode;
  title: string;
  detail: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-4 rounded-xl bg-sidebar p-4">
      <span className="rounded-lg bg-surface p-2 text-ink-soft">{icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-[14px] font-semibold">{title}</p>
        <p className="text-[13px] text-ink-soft">{detail}</p>
      </div>
      {children}
    </div>
  );
}

/** The running version, a manual check, and the install button when one is out. */
function UpdatesRow() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.appInfo });
  const recording = useRecorder().status !== null;
  const queryClient = useQueryClient();
  const { phase, install } = useInstall();
  const [result, setResult] = useState<{ update: Update | null } | { error: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const busy = phase.status === "downloading" || phase.status === "restarting";

  const checkNow = async () => {
    setChecking(true);
    try {
      const update = await check();
      setResult({ update });
      queryClient.setQueryData(["update"], update);
    } catch (e) {
      setResult({ error: String(e) });
    } finally {
      setChecking(false);
    }
  };
  const update = result && "update" in result ? result.update : null;

  return (
    <Row
      icon={<Download className="size-4" />}
      title={`Murmur ${info.data?.version ?? ""}`}
      detail={
        phase.status === "failed"
          ? `The update failed: ${phase.message}`
          : result === null
            ? "Murmur checks when it starts and every few hours."
            : "error" in result
              ? `Couldn't check: ${result.error}`
              : update
                ? `Version ${update.version} is available.`
                : "You have the latest version."
      }
    >
      {update ? (
        <button
          type="button"
          className="btn-primary"
          disabled={busy || recording}
          title={recording ? "Available once the recording ends" : undefined}
          onClick={() => void install(update)}
        >
          {recording ? "After the recording" : installLabel(phase)}
        </button>
      ) : (
        <button type="button" className="btn-ghost" disabled={checking} onClick={checkNow}>
          {checking ? "Checking…" : "Check for updates"}
        </button>
      )}
    </Row>
  );
}

function ModelPicker({
  provider,
  onChange,
}: {
  provider: Provider;
  onChange: (p: Provider) => void;
}) {
  const models = useQuery({
    queryKey: ["models", provider.kind],
    queryFn: () => ipc.listModels(provider),
    retry: false,
  });
  const ids = models.data?.map((m) => m.id) ?? [];
  const missing = models.data && provider.model && !ids.includes(provider.model);

  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[14px] font-semibold">Model</span>
      <div className="flex gap-2">
        <select
          className="input flex-1"
          value={provider.model}
          onChange={(e) => onChange({ ...provider, model: e.target.value })}
        >
          {!provider.model && <option value="">Choose a model</option>}
          {missing && <option value={provider.model}>{provider.model} (not available)</option>}
          {models.data?.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
        <button type="button" className="btn-ghost" onClick={() => models.refetch()}>
          Refresh
        </button>
      </div>
      {models.isLoading && <span className="text-[12px] text-ink-soft">Loading models…</span>}
      {models.error && <span className="text-[12px] text-accent">{String(models.error)}</span>}
      {missing && provider.kind === "ollama" && (
        <span className="text-[12px] text-ink-soft">
          Download it with <code>ollama pull {provider.model}</code>, then Refresh.
        </span>
      )}
    </label>
  );
}

function ApiKeyField({ name, saved }: { name: "anthropic" | "gemini"; saved: boolean }) {
  const queryClient = useQueryClient();
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const update = async (value: string | null) => {
    try {
      await ipc.setApiKey(name, value);
      setKey("");
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["api-keys"] });
      await queryClient.invalidateQueries({ queryKey: ["models", name] });
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Row
      icon={<KeyRound className="size-4" />}
      title="API key"
      detail={
        error ??
        (saved
          ? "Saved in the macOS Keychain. The app's UI never sees it."
          : "Stored in the macOS Keychain.")
      }
    >
      {saved ? (
        <button type="button" className="btn-ghost text-accent" onClick={() => update(null)}>
          Remove
        </button>
      ) : (
        <span className="flex gap-2">
          <input
            type="password"
            className="input w-56 bg-surface"
            placeholder="Paste your API key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
          <button
            type="button"
            className="btn-primary"
            disabled={!key.trim()}
            onClick={() => update(key)}
          >
            Save
          </button>
        </span>
      )}
    </Row>
  );
}

function Choice({
  selected,
  onClick,
  icon,
  name,
  detail,
}: {
  selected: boolean;
  onClick: () => void;
  icon: ReactNode;
  name: string;
  detail: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={selected}
      className={`flex flex-col gap-1.5 rounded-xl p-4 text-left ${
        selected ? "bg-accent-soft/50 ring-1 ring-accent/40" : "bg-sidebar hover:bg-muted"
      }`}
    >
      <span className="flex items-center gap-2 font-semibold">
        {icon} {name}
      </span>
      <span className="text-[13px] text-ink-soft">{detail}</span>
    </button>
  );
}

/** Keeps what the user typed (a trailing comma, a blank line) while storing the list. */
function VocabularyField({
  value,
  onChange,
}: {
  value: string[];
  onChange: (terms: string[]) => void;
}) {
  const [text, setText] = useState(() => value.join(", "));
  // Follow outside changes, like Discard.
  useEffect(() => {
    setText((t) =>
      JSON.stringify(parseVocabulary(t)) === JSON.stringify(value) ? t : value.join(", "),
    );
  }, [value]);
  return (
    <label className="flex flex-col gap-1.5">
      <span className="flex items-center gap-1.5 text-[14px] font-semibold">
        <SpellCheck className="size-4" /> Vocabulary
      </span>
      <textarea
        className="input min-h-[72px] resize-y bg-surface text-sm"
        placeholder="TKT, Mariana, Express Checkout, ready for QA"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          onChange(parseVocabulary(e.target.value));
        }}
      />
      <span className="text-[12px] text-ink-soft">
        Names, products and ticket prefixes Whisper should spell as written, separated by commas or
        lines. Names of remembered speakers are included automatically. Applies to new recordings
        and re-transcriptions.
      </span>
    </label>
  );
}

export function SettingsView() {
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: ipc.getSettings });
  const keys = useQuery({ queryKey: ["api-keys"], queryFn: ipc.apiKeys });
  const storage = useQuery({ queryKey: ["storage"], queryFn: ipc.storageUsage });
  const [draft, setDraft] = useState<Settings | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    if (settings.data && draft === null) setDraft(settings.data);
  }, [settings.data, draft]);

  if (!draft || !settings.data) return <div className="flex-1" />;
  const saved = settings.data;
  const llm = draft.llm;
  const set = (patch: Partial<Settings>) => {
    setDraft({ ...draft, ...patch });
    setMessage(null);
  };
  const cloud = llm.kind === "ollama" ? null : llm.kind;
  const changed = JSON.stringify(draft) !== JSON.stringify(saved);

  const apply = async () => {
    try {
      await ipc.updateSettings(draft);
      await Promise.all(
        [["settings"], ["app-info"], ["folders"], ["sessions"], ["session"]].map((queryKey) =>
          queryClient.invalidateQueries({ queryKey }),
        ),
      );
      setMessage({ ok: true, text: "Saved." });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={[...folderCrumbs(null), "Settings"]} />
      <div className="min-h-0 flex-1 overflow-y-auto px-8">
        <div className="mx-auto flex max-w-[860px] flex-col gap-10 pt-2 pb-16">
          <PageHeader
            eyebrow={
              <>
                <span>Configuration</span>
                <span className="text-outline">/</span>
                <span className="text-accent">This Mac</span>
              </>
            }
            title="Preferences & Engine"
            subtitle="Choose the AI for notes and questions, how speech is transcribed, and where files are written."
          />

          <Stage
            n={1}
            title="AI Synthesis Engine"
            description="Writes notes and answers questions about your meetings."
            badge={cloud ? undefined : "Air-gapped core"}
          >
            <div className="grid grid-cols-3 gap-3">
              {PROVIDERS.map((p) => (
                <Choice
                  key={p.kind}
                  selected={llm.kind === p.kind}
                  onClick={() =>
                    set({ llm: p.kind === saved.llm.kind ? saved.llm : DEFAULTS[p.kind] })
                  }
                  icon={p.icon}
                  name={p.name}
                  detail={p.detail}
                />
              ))}
            </div>
            {cloud && (
              <p className="rounded-lg bg-accent-soft px-4 py-3 text-[13px] text-on-accent-soft">
                Transcripts and questions will be sent to{" "}
                {cloud === "anthropic" ? "Anthropic" : "Google"}. Audio always stays on this Mac.
                Check your company's policy before using a cloud model for work meetings.
              </p>
            )}
            {cloud && <ApiKeyField name={cloud} saved={keys.data?.[cloud] ?? false} />}
            {(!cloud || keys.data?.[cloud]) && (
              <ModelPicker provider={llm} onChange={(p) => set({ llm: p })} />
            )}
            <Row
              icon={<FileText className="size-4" />}
              title="Write notes when a recording stops"
              detail="Summary, decisions, action items and open questions, from the transcript and your own notes."
            >
              <Toggle
                checked={draft.auto_notes}
                onChange={(auto_notes) => set({ auto_notes })}
                label="Write notes automatically"
              />
            </Row>
          </Stage>

          <Stage
            n={2}
            title="Whisper Speech & Speakers"
            description="Transcription and speaker separation run entirely on this Mac."
            badge="Apple Silicon"
          >
            <div className="grid grid-cols-2 gap-4">
              <div className="flex flex-col gap-1.5">
                <span className="text-[14px] font-semibold">Transcription</span>
                <span className="input">Whisper large-v3 turbo</span>
                <span className="text-[12px] text-ink-soft">
                  Drafts appear while people talk (Whisper base).
                </span>
              </div>
              <div className="flex flex-col gap-1.5">
                <span className="flex items-center gap-1.5 text-[14px] font-semibold">
                  <Languages className="size-4" /> Default languages
                </span>
                <div className="flex flex-wrap gap-1.5">
                  {LANGUAGES.map(([code, name]) => {
                    const on = draft.languages.includes(code);
                    return (
                      <button
                        key={code}
                        type="button"
                        aria-pressed={on}
                        onClick={() =>
                          set({
                            languages: on
                              ? draft.languages.filter((l) => l !== code)
                              : [...draft.languages, code],
                          })
                        }
                        className={`rounded-full px-2.5 py-0.5 text-[13px] font-medium ${
                          on ? "bg-accent text-on-accent" : "bg-muted hover:bg-high"
                        }`}
                      >
                        {name}
                      </button>
                    );
                  })}
                </div>
                <span className="text-[12px] text-ink-soft">
                  {languageHint(draft.languages.length)}
                </span>
              </div>
            </div>
            <label className="flex flex-col gap-1.5">
              <span className="flex items-center gap-1.5 text-[14px] font-semibold">
                <UserRound className="size-4" /> Your name
              </span>
              <input
                className="input bg-surface"
                placeholder="As people call you in meetings, e.g. Ana Torres"
                value={draft.user_name}
                onChange={(e) => set({ user_name: e.target.value })}
              />
              <span className="text-[12px] text-ink-soft">
                Action items given to this name, or to your first name, count as yours. Notes
                already say “Me” for what you take on.
              </span>
            </label>
            <VocabularyField
              value={draft.vocabulary}
              onChange={(vocabulary) => set({ vocabulary })}
            />
            <Row
              icon={<Radio className="size-4" />}
              title="Detect meetings"
              detail="When Teams, Slack, Zoom or a browser call starts using the microphone, Murmur offers to record. Nothing is recorded until you say so."
            >
              <Toggle
                checked={draft.detect_meetings}
                onChange={(detect_meetings) => set({ detect_meetings })}
                label="Offer to record detected meetings"
              />
            </Row>
            {draft.ignored_meeting_apps.length > 0 && (
              <Row
                icon={<BellOff className="size-4" />}
                title="Not offered for"
                detail="Calls in these apps don't get an offer. Remove one to be asked again."
              >
                <div className="flex flex-wrap justify-end gap-1.5">
                  {draft.ignored_meeting_apps.map((name) => (
                    <span key={name} className="chip gap-1 bg-muted py-1">
                      {name}
                      <button
                        type="button"
                        className="rounded-full p-0.5 text-ink-soft hover:bg-high hover:text-ink"
                        aria-label={`Ask again for ${name}`}
                        title="Ask again"
                        onClick={() =>
                          set({
                            ignored_meeting_apps: draft.ignored_meeting_apps.filter(
                              (a) => a !== name,
                            ),
                          })
                        }
                      >
                        <X className="size-3" />
                      </button>
                    </span>
                  ))}
                </div>
              </Row>
            )}
            <Row
              icon={<UsersRound className="size-4" />}
              title="Speaker identification"
              detail="After each meeting, call audio is split by voice (pyannote + TitaNet) and named from remembered voices."
            >
              <span className="chip bg-local-soft text-on-local-soft">Active</span>
            </Row>
            <Row
              icon={<ShieldCheck className="size-4" />}
              title="Hallucination filter"
              detail="Drops phrases Whisper invents on silence, like “Thanks for watching”."
            >
              <span className="chip bg-local-soft text-on-local-soft">Active</span>
            </Row>
          </Stage>

          <Stage
            n={3}
            title="Markdown Output & Vault"
            description="Every session is a real file you own, readable in Obsidian or any editor."
            badge="CommonMark"
          >
            <div className="flex flex-col gap-2">
              <span className="text-[14px] font-semibold">Default file format</span>
              <div className="grid grid-cols-2 gap-3">
                <Choice
                  selected={draft.default_format === "markdown"}
                  onClick={() => set({ default_format: "markdown" })}
                  icon={<Type className="size-4" />}
                  name="Markdown (.md)"
                  detail="Frontmatter, headings and checkboxes. Best for Obsidian."
                />
                <Choice
                  selected={draft.default_format === "txt"}
                  onClick={() => set({ default_format: "txt" })}
                  icon={<FileText className="size-4" />}
                  name="Plain text (.txt)"
                  detail="No formatting, readable anywhere."
                />
              </div>
            </div>
            <div className="flex flex-col gap-1.5">
              <span className="text-[14px] font-semibold">Vault folder</span>
              <div className="flex items-center gap-2">
                <span className="flex min-w-0 flex-1 items-center gap-2 rounded-lg bg-muted px-3 py-2">
                  <FolderOpen className="size-4 shrink-0 text-ink-soft" />
                  <span className="truncate text-sm" title={draft.vault_path}>
                    {draft.vault_path}
                  </span>
                </span>
                <button
                  type="button"
                  className="btn-ghost shrink-0"
                  onClick={async () => {
                    const folder = await ipc.pickVaultFolder();
                    if (folder) set({ vault_path: folder });
                  }}
                >
                  Choose…
                </button>
              </div>
              <span className="text-[12px] text-ink-soft">
                {draft.vault_path !== saved.vault_path
                  ? "When you apply, Murmur moves its notes and notebooks to this folder. Other files in the old folder stay where they are."
                  : "Plain files on disk, nothing in the cloud. Notebooks are folders inside it."}
              </span>
            </div>
            {storage.data && (
              <div className="flex items-start gap-3 rounded-lg bg-muted px-3 py-2.5">
                <AudioLines className="mt-0.5 size-4 shrink-0 text-ink-soft" />
                <p className="text-[13px] text-ink-soft">
                  <span className="font-semibold text-ink">
                    Recordings use {formatBytes(storage.data.audio_bytes)}
                  </span>{" "}
                  across {storage.data.sessions_with_audio}{" "}
                  {storage.data.sessions_with_audio === 1 ? "session" : "sessions"}. They're kept so
                  sessions can be re-transcribed; delete a session's audio from its ⋯ menu.
                </p>
              </div>
            )}
          </Stage>

          <Stage
            n={4}
            title="Updates"
            description="New versions come from Murmur's GitHub releases. Each one is checked against Murmur's signing key before it's installed."
          >
            <UpdatesRow />
          </Stage>
        </div>
      </div>

      <footer className="flex items-center justify-between bg-sidebar px-8 py-3">
        <span className={`text-[13px] ${message && !message.ok ? "text-accent" : "text-ink-soft"}`}>
          {message?.text ?? "Settings are saved on this Mac."}
        </span>
        <div className="flex gap-2">
          <button
            type="button"
            className="btn-ghost"
            disabled={!changed}
            onClick={() => {
              setDraft(saved);
              setMessage(null);
            }}
          >
            Discard changes
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={!changed || !llm.model}
            onClick={apply}
          >
            Apply changes
          </button>
        </div>
      </footer>
    </div>
  );
}
