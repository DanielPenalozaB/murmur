import { Radio, X } from "lucide-react";
import { useEffect, useState } from "react";
import { type DetectedMeeting, ipc } from "../../lib/ipc";
import { useNav, useUi } from "../../lib/nav";
import { defaultTitle } from "./StartDialog";

/** "Microsoft Teams meeting Oct 28, 03:30 PM"; a browser can't say which call it is. */
export function meetingTitle(meeting: DetectedMeeting, now = new Date()) {
  return defaultTitle(meeting.browser ? "Meeting" : `${meeting.app} meeting`, now);
}

/** Offers to record when a calling app starts using the microphone. */
export function MeetingBanner({ recording }: { recording: boolean }) {
  const [meeting, setMeeting] = useState<DetectedMeeting | null>(null);
  const go = useNav((s) => s.go);
  const setStartOpen = useUi((s) => s.setStartOpen);

  useEffect(() => {
    let active = true;
    const unlisten = ipc.onMeeting((m) => active && setMeeting(m));
    const unlistenRecord = ipc.onRecordMeeting((m) => {
      go({ kind: "live" });
      setStartOpen(true, meetingTitle(m));
    });
    // One may have been detected before this window loaded.
    ipc.detectedMeeting().then((m) => active && m && setMeeting(m));
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
      void unlistenRecord.then((stop) => stop());
    };
  }, [go, setStartOpen]);

  // A call to record only matters when idle; one that ended, while recording.
  if (!meeting || meeting.ended !== recording) return null;

  const dismiss = () => {
    setMeeting(null);
    void ipc.dismissMeeting();
  };

  return (
    // Floats over the view so the top bar (the window's drag area) stays put.
    <div
      role="status"
      className="fixed top-16 right-8 z-10 flex max-w-[460px] items-center gap-3 rounded-xl bg-accent-soft px-4 py-3 text-on-accent-soft shadow-xl"
    >
      <Radio className="size-4 shrink-0" />
      <p className="min-w-0 flex-1 text-[13px]">
        <span className="font-semibold">{meeting.app}</span>{" "}
        {meeting.ended
          ? "stopped using your microphone. Has the call ended?"
          : `is using your microphone. ${meeting.browser ? "In a call?" : "Record this meeting?"}`}
      </p>
      {meeting.ended ? (
        <button
          type="button"
          className="btn-primary"
          onClick={() => {
            setMeeting(null);
            void ipc.stopMeeting();
          }}
        >
          Stop recording
        </button>
      ) : (
        <button
          type="button"
          className="btn-primary"
          onClick={() => {
            dismiss();
            go({ kind: "live" });
            setStartOpen(true, meetingTitle(meeting));
          }}
        >
          Record
        </button>
      )}
      <button type="button" className="btn-quiet" aria-label="Dismiss" onClick={dismiss}>
        <X className="size-4" />
      </button>
    </div>
  );
}
