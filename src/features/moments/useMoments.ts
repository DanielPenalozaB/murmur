import { useEffect } from "react";
import { ipc } from "../../lib/ipc";
import { useNav } from "../../lib/nav";
import { useTranscript } from "../transcript/store";

/** Mentions, marks and quick notes from the backend go into the meeting's notes. */
export function useMoments() {
  useEffect(() => {
    const added = ipc.onMoment((moment) => useTranscript.getState().addMoment(moment));
    const shown = ipc.onShowMoment((at) => {
      useNav.getState().go({ kind: "live" });
      useTranscript.getState().showMoment(at);
    });
    return () => {
      for (const stop of [added, shown]) void stop.then((unlisten) => unlisten());
    };
  }, []);
}
