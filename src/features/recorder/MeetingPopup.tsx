import { Radio, X } from "lucide-react";
import { useEffect, useState } from "react";
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
    <div className="flex h-screen items-center gap-3 rounded-2xl bg-surface px-4 text-ink">
      <span className="rounded-lg bg-accent-soft p-2 text-on-accent-soft">
        <Radio className="size-4" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-semibold">
          {meeting.browser ? `Call in ${meeting.app}?` : `${meeting.app} meeting`}
        </p>
        <p className="text-[12px] text-ink-soft">Transcribe it with Murmur?</p>
      </div>
      <button type="button" className="btn-primary" onClick={() => void ipc.recordMeeting()}>
        Record
      </button>
      <button
        type="button"
        className="btn-quiet"
        aria-label="Dismiss"
        onClick={() => void ipc.dismissMeeting()}
      >
        <X className="size-4" />
      </button>
    </div>
  );
}
