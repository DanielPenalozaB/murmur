import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type Recording } from "../../lib/ipc";
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
    mutationFn: () => {
      transcript.reset();
      return ipc.startRecording(useTranscript.getState().handle);
    },
    onSettled: refresh,
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
    start: () => start.mutate(),
    togglePause: () => status.data && pause.mutate(!status.data.paused),
    stop: () => stop.mutate(),
    lastRecording: stop.data,
    // Loading the Whisper model on the first start takes a few seconds.
    starting: start.isPending,
    busy: start.isPending || stop.isPending,
    error: error ? String(error) : null,
  };
}
