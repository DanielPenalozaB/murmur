import { invoke } from "@tauri-apps/api/core";

// Hand-written until tauri-specta generates these (milestone 2).
export type Channel = "mic" | "system";

export type AppInfo = { version: string; llm: string; local: boolean };

export type RecorderStatus = {
  paused: boolean;
  elapsed_ms: number;
  mic_level: number;
  system_level: number;
};

export type Recording = {
  dir: string;
  tracks: { channel: Channel; path: string; duration_ms: number }[];
};

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  startRecording: () => invoke<void>("start_recording"),
  setRecordingPaused: (paused: boolean) => invoke<void>("set_recording_paused", { paused }),
  stopRecording: () => invoke<Recording>("stop_recording"),
  recordingStatus: () => invoke<RecorderStatus | null>("recording_status"),
};
