import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type NewSession, type Recording } from "../../lib/ipc";
import { useTranscript } from "../transcript/store";

const STATUS_KEY = ["recording-status"];

export function useRecorder() {
  const queryClient = useQueryClient();
  const transcript = useTranscript();
  const status = useQuery({
    queryKey: STATUS_KEY,
    queryFn: ipc.recordingStatus,
    // Poll fast enough for smooth meters, only while recording.
    refetchInterval: (query) => (query.state.data ? 100 : false),
  });
  const refresh = () => queryClient.invalidateQueries({ queryKey: STATUS_KEY });

  const start = useMutation({
    mutationFn: (session: NewSession) => {
      transcript.begin(session.title);
      return ipc.startRecording(session, (event) => {
        useTranscript.getState().handle(event);
        // A saved session changes folder counts and lists.
        if (event.kind === "finished") {
          queryClient.invalidateQueries({ queryKey: ["folders"] });
          queryClient.invalidateQueries({ queryKey: ["sessions"] });
        }
      });
    },
    onSettled: () => {
      refresh();
      // Start remembers the folder, format and languages for the next dialog.
      queryClient.invalidateQueries({ queryKey: ["settings"] });
    },
  });
  const pause = useMutation({ mutationFn: ipc.setRecordingPaused, onSettled: refresh });
  const stop = useMutation<Recording>({
    mutationFn: () => {
      transcript.setFinishing(true);
      return ipc.stopRecording();
    },
    onError: () => transcript.setFinishing(false),
    onSettled: refresh,
  });

  const error = start.error ?? pause.error ?? stop.error;
  return {
    status: status.data ?? null,
    start: (session: NewSession) => start.mutate(session),
    togglePause: () => status.data && pause.mutate(!status.data.paused),
    stop: () => stop.mutate(),
    starting: start.isPending,
    busy: start.isPending || stop.isPending,
    error: error ? String(error) : null,
  };
}
