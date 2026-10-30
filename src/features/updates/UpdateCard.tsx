import type { Update } from "@tauri-apps/plugin-updater";
import { Download, X } from "lucide-react";
import { useState } from "react";
import { Markdown } from "../assistant/Markdown";
import { Dialog } from "../library/manage";
import { installLabel, useInstall, useUpdate } from "./updates";

/** The release notes, with the same install button as the card. */
function WhatsNew({
  update,
  recording,
  onClose,
}: {
  update: Update;
  recording: boolean;
  onClose: () => void;
}) {
  const { phase, install } = useInstall();
  return (
    <Dialog
      icon={Download}
      eyebrow={`You have ${update.currentVersion}`}
      title={`What's new in ${update.version}`}
      submit={recording ? "Update after the recording" : installLabel(phase)}
      onSubmit={async () => {
        if (recording) return;
        await install(update);
      }}
      onClose={onClose}
    >
      <div className="max-h-[50vh] overflow-y-auto text-[14px]">
        <Markdown text={update.body?.trim() || "No release notes."} />
      </div>
      {phase.status === "failed" && (
        <p className="text-[13px] text-accent">The update failed: {phase.message}</p>
      )}
    </Dialog>
  );
}

/** A quiet card above the vault summary when a newer Murmur is out. */
export function UpdateCard({ recording }: { recording: boolean }) {
  const update = useUpdate();
  const { phase, install } = useInstall();
  const [hidden, setHidden] = useState(false);
  const [notes, setNotes] = useState(false);
  const available = update.data;
  if (!available || hidden) return null;
  const busy = phase.status === "downloading" || phase.status === "restarting";

  return (
    <div className="mb-2 flex flex-col gap-2 rounded-xl bg-accent-soft p-3 text-xs text-on-accent-soft">
      <div className="flex items-center justify-between gap-2">
        <span className="font-semibold">Murmur {available.version} is available</span>
        {!busy && (
          <button
            type="button"
            className="rounded-full p-0.5 opacity-70 hover:opacity-100"
            aria-label="Hide until next launch"
            title="Hide until next launch"
            onClick={() => setHidden(true)}
          >
            <X className="size-3.5" />
          </button>
        )}
      </div>
      {phase.status === "failed" && <p>The update failed: {phase.message}</p>}
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="btn-primary px-2.5 py-1 text-xs"
          disabled={busy || recording}
          title={recording ? "Available once the recording ends" : undefined}
          onClick={() => void install(available)}
        >
          {recording ? "After the recording" : installLabel(phase, "Update and restart")}
        </button>
        <button
          type="button"
          className="underline opacity-80 hover:opacity-100"
          onClick={() => setNotes(true)}
        >
          What's new
        </button>
      </div>
      {notes && (
        <WhatsNew update={available} recording={recording} onClose={() => setNotes(false)} />
      )}
    </div>
  );
}
