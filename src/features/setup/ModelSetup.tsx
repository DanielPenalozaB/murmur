import { useQueryClient } from "@tanstack/react-query";
import { Download, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { PageHeader, TopBar } from "../../components/ui";
import { ipc, type MissingModel } from "../../lib/ipc";
import { formatBytes } from "./format";

type Progress = { label: string; done: number; total: number };

/** Shown until the speech and speaker models are on this Mac. */
export function ModelSetup({ missing }: { missing: MissingModel[] }) {
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<Progress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const total = missing.reduce((sum, m) => sum + m.bytes, 0);

  const download = async () => {
    setDownloading(true);
    setError(null);
    try {
      await ipc.downloadModels((event) => {
        if (event.kind === "progress") setProgress(event);
      });
      await queryClient.invalidateQueries({ queryKey: ["missing-models"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const percent = progress ? Math.round((100 * progress.done) / progress.total) : 0;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar crumbs={["Vault", "Setup"]} />
      <div className="min-h-0 flex-1 overflow-y-auto px-8">
        <div className="mx-auto flex max-w-[640px] flex-col gap-8 pt-2 pb-16">
          <PageHeader
            eyebrow={<span className="text-accent">First run</span>}
            title="Download the speech models"
            subtitle="Murmur transcribes and tells voices apart entirely on this Mac. It needs these models once; after that, no audio or text leaves your computer unless you choose a cloud AI for notes."
          />
          <div className="card flex flex-col gap-5 p-6">
            <ul className="flex flex-col gap-2">
              {missing.map((m) => (
                <li key={m.label} className="flex items-center justify-between text-[14px]">
                  <span>{m.label}</span>
                  <span className="text-ink-soft">{formatBytes(m.bytes)}</span>
                </li>
              ))}
            </ul>
            {downloading ? (
              <div className="flex flex-col gap-2">
                <div
                  className="h-2 overflow-hidden rounded-full bg-muted"
                  role="progressbar"
                  aria-valuenow={percent}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="h-full bg-accent transition-all"
                    style={{ width: `${percent}%` }}
                  />
                </div>
                <p className="text-[13px] text-ink-soft">
                  {progress
                    ? `${progress.label}: ${formatBytes(progress.done)} of ${formatBytes(progress.total)}`
                    : "Starting…"}
                </p>
              </div>
            ) : (
              <button type="button" className="btn-primary self-start" onClick={download}>
                <Download className="size-4" /> Download {formatBytes(total)}
              </button>
            )}
            {error && (
              <p className="text-[13px] text-accent">
                {error}. Check your connection and try again; finished models are kept.
              </p>
            )}
          </div>
          <p className="flex items-center gap-2 text-[13px] text-ink-soft">
            <ShieldCheck className="size-4" /> Downloaded from Hugging Face (Whisper) and GitHub
            (sherpa-onnx) into ~/Library/Application Support/dev.murmur.app/models.
          </p>
        </div>
      </div>
    </div>
  );
}
