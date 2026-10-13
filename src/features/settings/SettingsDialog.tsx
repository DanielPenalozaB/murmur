import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { ipc, type Provider, type Settings } from "../../lib/ipc";

type Kind = Provider["kind"];

const DEFAULTS: Record<Kind, Provider> = {
  ollama: {
    kind: "ollama",
    base_url: "http://127.0.0.1:11434",
    model: "qwen3:14b",
    num_ctx: 32768,
  },
  anthropic: { kind: "anthropic", model: "claude-opus-5-5" },
  gemini: { kind: "gemini", model: "" },
};

const NAMES: Record<Kind, string> = {
  ollama: "Local (Ollama)",
  anthropic: "Claude",
  gemini: "Gemini",
};

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
    <div className="flex flex-col gap-1">
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
      {models.isLoading && <p className="text-xs text-ink-soft">Loading models…</p>}
      {models.error && <p className="text-xs text-accent">{String(models.error)}</p>}
      {missing && provider.kind === "ollama" && (
        <p className="text-xs text-ink-soft">
          Download it with <code>ollama pull {provider.model}</code>, then Refresh.
        </p>
      )}
    </div>
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
    <div className="flex flex-col gap-1">
      {saved ? (
        <div className="flex items-center justify-between rounded-lg border border-line px-3 py-1.5 text-sm">
          <span>API key saved in the Keychain</span>
          <button type="button" className="text-accent" onClick={() => update(null)}>
            Remove
          </button>
        </div>
      ) : (
        <div className="flex gap-2">
          <input
            type="password"
            className="input flex-1"
            placeholder="Paste your API key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
          <button
            type="button"
            className="btn-ghost"
            disabled={!key.trim()}
            onClick={() => update(key)}
          >
            Save key
          </button>
        </div>
      )}
      {error && <p className="text-xs text-accent">{error}</p>}
    </div>
  );
}

export function SettingsDialog({ settings, onClose }: { settings: Settings; onClose: () => void }) {
  const queryClient = useQueryClient();
  const keys = useQuery({ queryKey: ["api-keys"], queryFn: ipc.apiKeys });
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);
  const llm = draft.llm;
  const setLlm = (provider: Provider) => setDraft({ ...draft, llm: provider });
  const cloudKey = llm.kind === "ollama" ? null : llm.kind;

  const save = async () => {
    try {
      await ipc.updateSettings(draft);
      await Promise.all(
        [["settings"], ["app-info"]].map((queryKey) => queryClient.invalidateQueries({ queryKey })),
      );
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center">
      <button
        type="button"
        aria-label="Close"
        className="absolute inset-0 cursor-default bg-black/30"
        onClick={onClose}
      />
      <form
        className="relative flex max-h-[90vh] w-[520px] flex-col gap-5 overflow-y-auto rounded-2xl border border-line bg-surface p-6 shadow-xl"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <h2 className="font-serif text-2xl">Settings</h2>

        <fieldset className="flex flex-col gap-2 text-sm">
          <span className="text-ink-soft">AI for Ask and notes</span>
          <div className="flex gap-2">
            {(Object.keys(NAMES) as Kind[]).map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => setLlm(kind === settings.llm.kind ? settings.llm : DEFAULTS[kind])}
                className={`flex-1 rounded-lg border px-3 py-1.5 ${
                  llm.kind === kind ? "border-accent bg-accent/10 text-accent" : "border-line"
                }`}
              >
                {NAMES[kind]}
              </button>
            ))}
          </div>

          {cloudKey ? (
            <>
              <p className="rounded-lg bg-accent/10 px-3 py-2 text-xs text-accent">
                Transcripts and questions will be sent to{" "}
                {cloudKey === "anthropic" ? "Anthropic" : "Google"}. Audio always stays on this Mac.
                Check your company's policy before using a cloud model for work meetings.
              </p>
              <ApiKeyField name={cloudKey} saved={keys.data?.[cloudKey] ?? false} />
              {keys.data?.[cloudKey] && <ModelPicker provider={llm} onChange={setLlm} />}
            </>
          ) : (
            <>
              <p className="text-xs text-ink-soft">
                Runs on this Mac. Nothing leaves your computer.
              </p>
              <ModelPicker provider={llm} onChange={setLlm} />
            </>
          )}
        </fieldset>

        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="accent-accent"
            checked={draft.auto_notes}
            onChange={(e) => setDraft({ ...draft, auto_notes: e.target.checked })}
          />
          Write notes automatically when a recording stops
        </label>

        <label className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">Vault folder (where session files are saved)</span>
          <input
            className="input"
            value={draft.vault_path}
            onChange={(e) => setDraft({ ...draft, vault_path: e.target.value })}
          />
        </label>

        {error && <p className="text-sm text-accent">{error}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" className="btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="btn-primary" disabled={!llm.model}>
            Save
          </button>
        </div>
      </form>
    </div>
  );
}
