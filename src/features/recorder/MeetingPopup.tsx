import { Circle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { LogoMark } from "../../components/ui";
import { type DetectedMeeting, ipc } from "../../lib/ipc";

/** The card over other apps: Record brings Murmur forward with the New session dialog. */
export function MeetingPopup() {
  const [meeting, setMeeting] = useState<DetectedMeeting | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = ipc.onMeeting((m) => active && setMeeting(m));
    ipc.detectedMeeting().then((m) => active && setMeeting(m));
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!meeting) return null;

  return (
    // The window is sized to this card (see POPUP_SIZE in src-tauri/src/meeting.rs).
    <div className="flex h-screen items-center gap-2.5 rounded-xl bg-surface pr-2 pl-3 text-ink ring-1 ring-line ring-inset">
      <LogoMark size="sm" />
      <div className="min-w-0 flex-1 leading-tight">
        <p className="truncate text-[13px] font-semibold">
          {meeting.browser ? `Call in ${meeting.app}?` : `${meeting.app} meeting`}
        </p>
        <p className="text-[11px] text-ink-soft">Using your microphone</p>
      </div>
      <button
        type="button"
        className="inline-flex items-center gap-1.5 rounded-md bg-accent px-2.5 py-1 text-[12px] font-semibold text-on-accent hover:opacity-90"
        onClick={() => void ipc.recordMeeting()}
      >
        <Circle className="size-2.5 fill-current" /> Record
      </button>
      <button
        type="button"
        className="rounded-md p-1 text-ink-soft hover:bg-muted hover:text-ink"
        aria-label="Dismiss"
        onClick={() => void ipc.dismissMeeting()}
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
