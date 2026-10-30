import { Circle, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";
import { LogoMark } from "../../components/ui";
import { type DetectedMeeting, ipc } from "../../lib/ipc";

const small =
  "rounded-md px-2 py-1 text-[12px] font-semibold text-ink-soft hover:bg-muted hover:text-ink";

/** The card over other apps: Record brings Murmur forward with the New session dialog. */
export function MeetingPopup({ initiallyCollapsed = false }: { initiallyCollapsed?: boolean }) {
  const [meeting, setMeeting] = useState<DetectedMeeting | null>(null);
  const [collapsed, setCollapsed] = useState(initiallyCollapsed);
  // The ✕ asks whether to hide this call's offer or stop asking about the app.
  const [hiding, setHiding] = useState(false);

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

  // The window is resized to fit before the card changes (see collapse_meeting_popup).
  const collapse = async (value: boolean) => {
    await ipc.collapseMeetingPopup(value);
    setCollapsed(value);
  };
  const label = meeting.ended
    ? `${meeting.app} call ended?`
    : meeting.browser
      ? `Call in ${meeting.app}?`
      : `${meeting.app} meeting`;

  if (collapsed) {
    return (
      <button
        type="button"
        className="relative flex h-screen w-screen items-center justify-center rounded-xl bg-surface ring-1 ring-line ring-inset hover:bg-muted"
        title={`${label} Click to show.`}
        aria-label={`${label} Show the offer`}
        onClick={() => void collapse(false)}
      >
        <LogoMark size="sm" />
        <span className="absolute top-2 right-2 size-2 animate-pulse rounded-full bg-accent" />
      </button>
    );
  }

  return (
    // The window is sized to this card (see POPUP_SIZE in src-tauri/src/meeting.rs).
    <div className="flex h-screen items-center gap-2 rounded-xl bg-surface pr-2 pl-3 text-ink ring-1 ring-line ring-inset">
      <LogoMark size="sm" />
      {hiding ? (
        <>
          <div className="min-w-0 flex-1 leading-tight">
            <p className="truncate text-[13px] font-semibold">Stop asking?</p>
            <p className="truncate text-[11px] text-ink-soft">For {meeting.app} calls</p>
          </div>
          <button type="button" className={small} onClick={() => void ipc.dismissMeeting()}>
            This call
          </button>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[12px] font-semibold text-accent ring-1 ring-accent/40 hover:bg-accent-soft"
            title="Settings can undo this"
            onClick={() => void ipc.ignoreMeetingApp(meeting.app)}
          >
            Always
          </button>
        </>
      ) : (
        <>
          <div className="min-w-0 flex-1 leading-tight">
            <p className="truncate text-[13px] font-semibold">{label}</p>
            <p className="text-[11px] text-ink-soft">
              {meeting.ended ? "Murmur is still recording" : "Using your microphone"}
            </p>
          </div>
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-2.5 py-1 text-[12px] font-semibold text-on-accent hover:opacity-90"
            onClick={() => void (meeting.ended ? ipc.stopMeeting() : ipc.recordMeeting())}
          >
            {meeting.ended ? (
              <>
                <Square className="size-2.5 fill-current" /> Stop
              </>
            ) : (
              <>
                <Circle className="size-2.5 fill-current" /> Record
              </>
            )}
          </button>
          <button
            type="button"
            className="rounded-md p-1 text-ink-soft hover:bg-muted hover:text-ink"
            aria-label="Collapse"
            title="Shrink to the logo"
            onClick={() => void collapse(true)}
          >
            <Minus className="size-3.5" />
          </button>
          <button
            type="button"
            className="rounded-md p-1 text-ink-soft hover:bg-muted hover:text-ink"
            aria-label="Dismiss"
            onClick={() => (meeting.ended ? void ipc.dismissMeeting() : setHiding(true))}
          >
            <X className="size-3.5" />
          </button>
        </>
      )}
    </div>
  );
}
