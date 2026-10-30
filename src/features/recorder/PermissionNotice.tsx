import { useQuery } from "@tanstack/react-query";
import { ShieldAlert } from "lucide-react";
import { ipc, type Permissions } from "../../lib/ipc";

const SOURCES = [
  { source: "mic", what: "your microphone", pane: "Microphone" },
  { source: "system", what: "the call's audio", pane: "Screen & System Audio Recording" },
] as const;

/** Sources macOS refuses. Not asked yet is fine: macOS asks when recording starts. */
export function denied(permissions: Permissions | null | undefined) {
  return SOURCES.filter((s) => permissions?.[s.source] === "denied");
}

/**
 * Says which permission is missing, before a silent recording would. Checked again
 * whenever the window comes back to the front, e.g. from System Settings.
 */
export function PermissionNotice() {
  const permissions = useQuery({ queryKey: ["permissions"], queryFn: ipc.permissions });
  const missing = denied(permissions.data);
  if (missing.length === 0) return null;
  return (
    <div className="mx-8 mb-3 flex flex-col gap-2 rounded-lg bg-accent-soft px-4 py-3 text-[13px] text-on-accent-soft">
      {missing.map((s) => (
        <div key={s.source} className="flex items-center gap-3">
          <ShieldAlert className="size-4 shrink-0" />
          <p className="flex-1">
            <span className="font-semibold">macOS isn't letting Murmur record {s.what}.</span> Turn
            Murmur on in Privacy &amp; Security → {s.pane}
            {s.source === "system" ? " (System Audio Recording Only)" : ""}. When running from a
            terminal, turn on the terminal instead.
          </p>
          <button
            type="button"
            className="btn-ghost shrink-0 bg-surface/60 py-1"
            onClick={() => ipc.openPrivacySettings(s.source)}
          >
            Open System Settings
          </button>
        </div>
      ))}
    </div>
  );
}
