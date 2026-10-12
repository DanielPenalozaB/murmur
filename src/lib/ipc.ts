import { Channel as IpcChannel, invoke } from "@tauri-apps/api/core";

// Hand-written for now; generate with tauri-specta once the command surface settles.
export type Channel = "mic" | "system";
export type ExportFormat = "markdown" | "txt";

export type AppInfo = { version: string; llm: string; local: boolean };

export type Settings = {
  vault_path: string;
  default_format: ExportFormat;
  last_folder: string;
};

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

export type SessionRecord = {
  id: string;
  title: string;
  folder: string;
  started_at: string;
  duration_ms: number;
  format: ExportFormat;
  file_path: string;
};

export type SessionDetail = SessionRecord & { segments: Segment[] };

export type FolderNode = { name: string; path: string; children: FolderNode[] };
export type Folders = { tree: FolderNode[]; counts: Record<string, number> };

export type SearchHit = {
  session_id: string;
  title: string;
  folder: string;
  started_at: string;
  start: number;
  snippet: string;
};

export type TranscriptEvent =
  | { kind: "segment"; segment: Segment }
  | { kind: "error"; message: string }
  | { kind: "finished"; session: SessionRecord | null };

export type NewSession = { title: string; folder: string; format: ExportFormat };

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<void>("update_settings", { settings }),

  listFolders: () => invoke<Folders>("list_folders"),
  createFolder: (path: string) => invoke<void>("create_folder", { path }),
  listSessions: (folder: string | null) => invoke<SessionRecord[]>("list_sessions", { folder }),
  getSession: (id: string) => invoke<SessionDetail>("get_session", { id }),
  search: (query: string) => invoke<SearchHit[]>("search", { query }),
  openSessionItem: (id: string, target: "open_file" | "reveal_file" | "reveal_audio") =>
    invoke<void>("open_session_item", { id, target }),

  startRecording: (session: NewSession, onEvent: (event: TranscriptEvent) => void) => {
    const channel = new IpcChannel<TranscriptEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("start_recording", { ...session, onEvent: channel });
  },
  setRecordingPaused: (paused: boolean) => invoke<void>("set_recording_paused", { paused }),
  stopRecording: () => invoke<Recording>("stop_recording"),
  recordingStatus: () => invoke<RecorderStatus | null>("recording_status"),
};

export function formatDate(iso: string) {
  return new Date(iso).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function formatDuration(ms: number) {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 1) return "<1 min";
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}
