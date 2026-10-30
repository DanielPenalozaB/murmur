import { Channel as IpcChannel, invoke } from "@tauri-apps/api/core";

// Hand-written for now; generate with tauri-specta once the command surface settles.
export type Channel = "mic" | "system";
export type ExportFormat = "markdown" | "txt";

export type AppInfo = { version: string; llm: string; local: boolean };

export type Provider =
  | { kind: "ollama"; base_url: string; model: string; num_ctx: number }
  | { kind: "anthropic"; model: string }
  | { kind: "gemini"; model: string };

export type ModelInfo = { id: string; label: string };
/** A speech or speaker model still to download. */
export type MissingModel = { label: string; bytes: number };
export type DownloadEvent =
  | { kind: "progress"; label: string; done: number; total: number }
  | { kind: "finished" };
export type ApiKeys = { anthropic: boolean; gemini: boolean };

export type StreamEvent =
  | { kind: "text"; text: string }
  | { kind: "done"; text: string }
  | { kind: "error"; message: string };

export type Settings = {
  vault_path: string;
  default_format: ExportFormat;
  last_folder: string;
  /** ISO codes. Empty: detect per chunk. One: forced. Several: follow switches. */
  languages: string[];
  llm: Provider;
  auto_notes: boolean;
  /** Names and terms hinted to Whisper. Remembered speakers are added automatically. */
  vocabulary: string[];
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
  /** The user turned their mic off in Murmur. */
  mic_muted: boolean;
  restarts: number;
  /** The mic sends pure digital silence: muted on the headset, or not delivering. */
  mic_silent: boolean;
};

export type RetranscribeEvent =
  | { kind: "progress"; done: number; total: number }
  | { kind: "status"; message: string }
  | { kind: "finished"; segments: number; speakers: number };

export type Recording = {
  dir: string;
  tracks: { channel: Channel; path: string; duration_ms: number; gain: number }[];
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

export type SpeakerInfo = { id: string; name: string; named: boolean };

export type SessionDetail = SessionRecord & {
  segments: Segment[];
  notes: string;
  speakers: SpeakerInfo[];
  /** Size of the recordings; 0 once deleted, and then it can't be re-transcribed. */
  audio_bytes: number;
};

export type StorageUsage = { audio_bytes: number; sessions_with_audio: number };

/** A session as a library card shows it. */
export type SessionCard = SessionRecord & {
  excerpt: string;
  actions_open: number;
  actions_done: number;
  /** People on the call, in speaking order. */
  speakers: string[];
};

export type VoiceEntry = { name: string; meetings: number; last_seen: string | null };

export type FolderNode = { name: string; path: string; children: FolderNode[] };
export type Folders = { tree: FolderNode[]; counts: Record<string, number> };

export type SearchHit = {
  session_id: string;
  title: string;
  folder: string;
  started_at: string;
  start: number;
  /** "Me", a speaker's name, "Speaker 2" or "Them". */
  speaker: string;
  snippet: string;
};

export type TranscriptEvent =
  | { kind: "segment"; segment: Segment }
  | { kind: "draft"; channel: Channel; start: number; text: string }
  | { kind: "draft_done"; channel: Channel; until: number }
  | { kind: "status"; message: string }
  | { kind: "error"; message: string }
  | {
      kind: "finished";
      session: SessionRecord | null;
      segments: Segment[];
      speakers: SpeakerInfo[];
    };

export type NewSession = {
  title: string;
  folder: string;
  format: ExportFormat;
  languages: string[];
};

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<void>("update_settings", { settings }),

  listFolders: () => invoke<Folders>("list_folders"),
  createFolder: (path: string) => invoke<void>("create_folder", { path }),
  moveFolder: (from: string, to: string) => invoke<void>("move_folder", { from, to }),
  deleteFolder: (folder: string) => invoke<void>("delete_folder", { folder }),
  renameSession: (id: string, title: string) => invoke<void>("rename_session", { id, title }),
  moveSession: (id: string, folder: string) => invoke<void>("move_session", { id, folder }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),
  deleteSessionAudio: (id: string) => invoke<void>("delete_session_audio", { id }),
  storageUsage: () => invoke<StorageUsage>("storage_usage"),
  syncVault: () => invoke<number>("sync_vault"),
  listSessions: (folder: string | null) => invoke<SessionCard[]>("list_sessions", { folder }),
  getSession: (id: string) => invoke<SessionDetail>("get_session", { id }),
  search: (query: string) => invoke<SearchHit[]>("search", { query }),
  openSessionItem: (id: string, target: "open_file" | "reveal_file" | "reveal_audio") =>
    invoke<void>("open_session_item", { id, target }),

  startRecording: (session: NewSession, onEvent: (event: TranscriptEvent) => void) => {
    const channel = new IpcChannel<TranscriptEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("start_recording", { ...session, onEvent: channel });
  },
  /** `sessionId` null asks about the recording in progress. */
  ask: (sessionId: string | null, question: string, onEvent: (event: StreamEvent) => void) =>
    invoke<void>("ask", { sessionId, question, onEvent: streamChannel(onEvent) }),
  generateNotes: (sessionId: string, userNotes: string, onEvent: (event: StreamEvent) => void) =>
    invoke<void>("generate_notes", { sessionId, userNotes, onEvent: streamChannel(onEvent) }),
  saveNotes: (sessionId: string, notes: string) => invoke<void>("save_notes", { sessionId, notes }),
  listModels: (provider: Provider) => invoke<ModelInfo[]>("list_models", { provider }),
  apiKeys: () => invoke<ApiKeys>("api_keys"),
  setApiKey: (name: "anthropic" | "gemini", key: string | null) =>
    invoke<void>("set_api_key", { name, key }),

  renameSpeaker: (sessionId: string, speakerId: string, name: string) =>
    invoke<void>("rename_speaker", { sessionId, speakerId, name }),
  listVoices: () => invoke<VoiceEntry[]>("list_voices"),
  forgetVoice: (name: string) => invoke<void>("forget_voice", { name }),
  sessionsWithSpeaker: (name: string) => invoke<SessionRecord[]>("sessions_with_speaker", { name }),

  retranscribeSession: (
    id: string,
    languages: string[],
    onEvent: (event: RetranscribeEvent) => void,
  ) => {
    const channel = new IpcChannel<RetranscribeEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("retranscribe_session", { id, languages, onEvent: channel });
  },
  missingModels: () => invoke<MissingModel[]>("missing_models"),
  downloadModels: (onEvent: (event: DownloadEvent) => void) => {
    const channel = new IpcChannel<DownloadEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("download_models", { onEvent: channel });
  },
  sessionLanguages: (id: string) => invoke<string[] | null>("session_languages", { id }),

  setRecordingPaused: (paused: boolean) => invoke<void>("set_recording_paused", { paused }),
  setMicMuted: (muted: boolean) => invoke<void>("set_mic_muted", { muted }),
  stopRecording: () => invoke<Recording>("stop_recording"),
  recordingStatus: () => invoke<RecorderStatus | null>("recording_status"),
};

function streamChannel(onEvent: (event: StreamEvent) => void) {
  const channel = new IpcChannel<StreamEvent>();
  channel.onmessage = onEvent;
  return channel;
}

export function formatDate(iso: string) {
  return new Date(iso).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** "Today, 10:30 AM", "Yesterday, 3:15 PM" or "Oct 24, 2026". */
export function formatRelative(iso: string, now = new Date()) {
  const date = new Date(iso);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(now) - day(date)) / 86_400_000);
  const time = date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  if (days === 0) return `Today, ${time}`;
  if (days === 1) return `Yesterday, ${time}`;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/** "240 MB", "2.8 GB". */
export function formatBytes(bytes: number) {
  if (bytes < 1e6) return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
  if (bytes < 1e9) return `${Math.round(bytes / 1e6)} MB`;
  return `${(bytes / 1e9).toFixed(1)} GB`;
}

export function formatDuration(ms: number) {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 1) return "<1 min";
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}
