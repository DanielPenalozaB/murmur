import { useEffect, useRef } from "react";
import {
  type DetectedMeeting,
  ipc,
  type NewSession,
  type Settings,
  type TrayAction,
} from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { meetingTitle } from "./MeetingBanner";
import { defaultTitle } from "./StartDialog";
import { useRecorder } from "./useRecorder";

/** Start from the menu bar skips the dialog: the last choices, titled after the call. */
export function traySession(
  settings: Settings,
  meeting: DetectedMeeting | null,
  now = new Date(),
): NewSession {
  return {
    title: meeting ? meetingTitle(meeting, now) : defaultTitle("Meeting", now),
    folder: settings.last_folder,
    format: settings.default_format,
    languages: settings.languages,
  };
}

/** Runs menu-bar items, which work with the window closed. */
export function useTrayActions() {
  const recorder = useRecorder();
  // The listener is registered once; it reads the latest recorder through this.
  const latest = useRef(recorder);
  latest.current = recorder;

  useEffect(() => {
    const run = async (action: TrayAction) => {
      const r = latest.current;
      switch (action) {
        case "start": {
          const [settings, meeting] = await Promise.all([ipc.getSettings(), ipc.detectedMeeting()]);
          if (meeting) await ipc.dismissMeeting();
          // Opening the window later shows the recording in progress.
          useNav.getState().go({ kind: "live" });
          r.start(traySession(settings, meeting));
          break;
        }
        case "pause":
        case "resume":
          r.setPaused(action === "pause");
          break;
        case "mic-off":
        case "mic-on":
          r.setMicMuted(action === "mic-off");
          break;
        case "stop":
          r.stop();
          break;
      }
    };
    const unlisten = ipc.onTrayAction((action) => void run(action));
    return () => void unlisten.then((stop) => stop());
  }, []);
}
