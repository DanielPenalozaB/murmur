import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type Recording } from "../../lib/ipc";

const STATUS_KEY = ["recording-status"];

export function useRecorder() {
  const queryClient = useQueryClient();
  const status = useQuery({
    queryKey: STATUS_KEY,
    queryFn: ipc.recordingStatus,
    // Poll fast enough for smooth meters, only while recording.
    refetchInterval: (query) => (query.state.data ? 100 : false),
  });
  const refresh = () => queryClient.invalidateQueries({ queryKey: STATUS_KEY });

  const start = useMutation({ mutationFn: ipc.startRecording, onSettled: refresh });
  const pause = useMutation({ mutationFn: ipc.setRecordingPaused, onSettled: refresh });
  const stop = useMutation<Recording>({ mutationFn: ipc.stopRecording, onSettled: refresh });

  const error = start.error ?? pause.error ?? stop.error;
  return {
    status: status.data ?? null,
    start: () => start.mutate(),
    togglePause: () => status.data && pause.mutate(!status.data.paused),
    stop: () => stop.mutate(),
    lastRecording: stop.data,
    busy: start.isPending || stop.isPending,
    error: error ? String(error) : null,
  };
}
