import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Cloud,
  Cpu,
  FileText,
  FolderOpen,
  KeyRound,
  Languages,
  ShieldCheck,
  SpellCheck,
  Type,
  UsersRound,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { PageHeader, Toggle, TopBar } from "../../components/ui";
import { ipc, type Provider, type Settings } from "../../lib/ipc";
import { languageHint } from "../recorder/StartDialog";
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
        [["settings"], ["app-info"], ["folders"]].map((queryKey) =>
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
      <TopBar crumbs={["Vault", "Settings"]} />
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
            <VocabularyField
              value={draft.vocabulary}
              onChange={(vocabulary) => set({ vocabulary })}
            />
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
            <label className="flex flex-col gap-1.5">
              <span className="text-[14px] font-semibold">Vault folder</span>
              <span className="flex items-center gap-2 rounded-lg bg-muted px-3">
                <FolderOpen className="size-4 text-ink-soft" />
                <input
                  className="flex-1 bg-transparent py-2 text-sm outline-none"
                  value={draft.vault_path}
                  onChange={(e) => set({ vault_path: e.target.value })}
                />
              </span>
              <span className="text-[12px] text-ink-soft">
                Plain files on disk, nothing in the cloud. Notebooks are folders inside it.
              </span>
            </label>
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
