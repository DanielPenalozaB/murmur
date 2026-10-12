import { Channel as IpcChannel, invoke } from "@tauri-apps/api/core";

// Hand-written for now; generate with tauri-specta once the command surface settles.
export type Channel = "mic" | "system";

export type AppInfo = { version: string; llm: string; local: boolean };

export type RecorderStatus = {
  paused: boolean;
  elapsed_ms: number;
  uptime_ms: number;
  mic_level: number;
  system_level: number;
  mic_speaking: boolean;
  system_speaking: boolean;
  mic_receiving: boolean;
  system_receiving: boolean;
};

export type Recording = {
  dir: string;
  tracks: { channel: Channel; path: string; duration_ms: number }[];
};

export type Segment = {
  start: number;
  end: number;
  channel: Channel;
  speaker: string | null;
  text: string;
};

export type TranscriptEvent =
  | { kind: "segment"; segment: Segment }
  | { kind: "error"; message: string }
  | { kind: "finished" };

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  startRecording: (onEvent: (event: TranscriptEvent) => void) => {
    const channel = new IpcChannel<TranscriptEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("start_recording", { onEvent: channel });
  },
  setRecordingPaused: (paused: boolean) => invoke<void>("set_recording_paused", { paused }),
  stopRecording: () => invoke<Recording>("stop_recording"),
  recordingStatus: () => invoke<RecorderStatus | null>("recording_status"),
};
