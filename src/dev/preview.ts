// Dev-only: renders the UI in a plain browser with Tauri's IPC mocked, so screens can
// be checked against the Stitch designs without the native app. Open the Vite URL with
// `?preview&view=live|live-muted|live-silent|permissions|setup|empty|start|meeting|popup|popup-ended|library|session|retranscribing|search|speakers|settings`.
import { mockIPC } from "@tauri-apps/api/mocks";
import { useAsk } from "../features/assistant/store";
import { useRetranscribe } from "../features/library/retranscribe";
import { useTranscript } from "../features/transcript/store";
import type { Segment, SessionCard, SessionDetail, Settings } from "../lib/ipc";
import { useNav, useUi } from "../lib/nav";

const iso = (daysAgo: number, h: number, m: number) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  return d.toISOString();
};

const seg = (
  start: number,
  channel: Segment["channel"],
  speaker: string | null,
  text: string,
): Segment => ({
  start,
  end: start + 5000,
  channel,
  speaker,
  text,
});

const segments: Segment[] = [
  seg(
    4_000,
    "system",
    "speaker_1",
    "Good morning everyone, let's start with the deployment status for this sprint.",
  ),
  seg(
    11_000,
    "system",
    "speaker_2",
    "Buenos días. El despliegue está casi listo, solo falta la revisión de seguridad.",
  ),
  seg(19_000, "mic", null, "Do we know when the security review will be finished?"),
  seg(
    26_000,
    "system",
    "speaker_1",
    "I talked to the security team yesterday, they expect to finish on Thursday.",
  ),
  seg(
    34_000,
    "system",
    "speaker_2",
    "Perfecto, entonces podemos lanzar el viernes por la mañana si todo sale bien.",
  ),
  seg(
    43_000,
    "mic",
    null,
    "Sounds good. I'll update the release notes and send them to the product team.",
  ),
  seg(
    52_000,
    "system",
    "speaker_1",
    "Great. Let's also make sure the rollback plan is documented before Friday.",
  ),
];

const notes = `## Summary
- Deployment is ready except for the security review, expected Thursday [00:26].
- Launch planned for **Friday morning** if the review passes [00:34].
- The rollback plan must be documented before launch [00:52].

## Decisions
- Launch on Friday morning, pending the security sign-off [00:34]
- Release notes go to the product team before launch [00:43]

## Action items
- [ ] Me: update the release notes and send them to the product team [00:43]
- [ ] Ana: document the rollback plan before Friday [00:52]
- [x] Bea: confirm the security review date [00:26]

## Open questions
- What happens if the security review slips past Thursday?

## My notes

check rollback owner`;

const speakers = [
  { id: "speaker_1", name: "Ana Torres", named: true },
  { id: "speaker_2", name: "Speaker 2", named: false },
  { id: "speaker_3", name: "Speaker 3", named: false, suggestion: "Marcus Chen" },
];

const record = {
  id: "2026-10-13_10-30-00",
  title: "Sprint Release Review",
  folder: "Work",
  started_at: iso(0, 10, 30),
  duration_ms: 48 * 60_000,
  format: "markdown" as const,
  file_path: "/Users/me/Documents/Murmur/Work/2026-10-13 Sprint Release Review.md",
};

const detail: SessionDetail = { ...record, segments, notes, speakers, audio_bytes: 241_000_000 };

const cards: SessionCard[] = [
  {
    ...record,
    excerpt: "Deployment is ready except for the security review, expected Thursday.",
    actions_open: 2,
    actions_done: 1,
    speakers: ["Ana Torres", "Speaker 2"],
  },
  {
    ...record,
    id: "b",
    title: "1:1 with Sarah",
    folder: "1-1s",
    started_at: iso(1, 15, 15),
    duration_ms: 25 * 60_000,
    excerpt: "Quarterly check-in: Sarah wants to own the transcription pipeline next quarter.",
    actions_open: 0,
    actions_done: 2,
    speakers: ["Sarah Lin"],
  },
  {
    ...record,
    id: "c",
    title: "Architecture sync — caching strategy",
    started_at: iso(9, 9, 0),
    duration_ms: 52 * 60_000,
    excerpt: "Agreed to move the cache in-process; David benchmarks the new tier by Thursday.",
    actions_open: 3,
    actions_done: 0,
    speakers: ["David Kim", "Ana Torres", "Marcus Chen"],
  },
  {
    ...record,
    id: "d",
    title: "All-hands Q&A",
    started_at: iso(12, 16, 0),
    duration_ms: 64 * 60_000,
    excerpt: "Roadmap review and open questions from every team.",
    actions_open: 4,
    actions_done: 0,
    // A long list must truncate, not widen the column.
    speakers: Array.from({ length: 50 }, (_, i) => `Speaker ${i + 1}`),
  },
];

const settings: Settings = {
  vault_path: "/Users/me/Documents/Murmur",
  default_format: "markdown",
  last_folder: "Work",
  languages: ["en", "es"],
  llm: { kind: "ollama", base_url: "http://127.0.0.1:11434", model: "qwen3:14b", num_ctx: 32768 },
  auto_notes: true,
  vocabulary: ["TKT", "Mariana", "Tomas", "Express Checkout"],
  detect_meetings: true,
};

export function install() {
  const params = new URLSearchParams(location.search);
  const view = params.get("view") ?? "live";
  const recording = view === "live" || view === "live-muted" || view === "live-silent";
  let micMuted = view === "live-muted";
  // "start" shows the New session dialog; "empty" the idle Live screen.
  if (view === "start") useUi.setState({ startOpen: true });

  mockIPC((cmd, args) => {
    switch (cmd) {
      case "app_info":
        return { version: "0.1.0", llm: "Local · qwen3:14b", local: true };
      case "get_settings":
        return settings;
      case "list_folders":
        return {
          tree: [
            { name: "Work", path: "Work", children: [] },
            { name: "1-1s", path: "1-1s", children: [] },
            { name: "Personal", path: "Personal", children: [] },
          ],
          counts: { Work: 5, "1-1s": 3, Personal: 1 },
        };
      case "list_sessions":
        return cards;
      case "get_session":
        return detail;
      case "permissions":
        return view === "permissions"
          ? { mic: "granted", system: "denied" }
          : { mic: "granted", system: "granted" };
      case "pick_vault_folder":
        return "/Users/me/Obsidian/Murmur";
      case "storage_usage":
        return { audio_bytes: 2_810_000_000, sessions_with_audio: 23 };
      case "list_voices":
        return [
          { name: "Ana Torres", meetings: 14, last_seen: iso(0, 10, 30) },
          { name: "David Kim", meetings: 9, last_seen: iso(9, 9, 0) },
          { name: "Sarah Lin", meetings: 6, last_seen: iso(1, 15, 15) },
          { name: "Marcus Chen", meetings: 4, last_seen: iso(20, 11, 0) },
          { name: "Lena Fischer", meetings: 3, last_seen: iso(2, 14, 0) },
          { name: "Omar Haddad", meetings: 2, last_seen: iso(6, 9, 30) },
        ];
      // Off unless `&review` is in the URL: a tidy library has nothing to review, and
      // that's how the README shows the directory.
      case "speaker_review": {
        if (!params.has("review")) return [];
        const at = (id: string, title: string, daysAgo: number, speaker_id: string) => ({
          session_id: id,
          title,
          started_at: iso(daysAgo, 10, 30),
          speaker_id,
          label: `Speaker ${speaker_id.slice(8)}`,
        });
        return [
          {
            kind: "pair",
            key: "pair",
            name: "David Kim",
            score: 0.31,
            speakers: [
              at(record.id, record.title, 0, "speaker_4"),
              at(record.id, record.title, 0, "speaker_7"),
            ],
          },
          {
            kind: "outlier",
            key: "outlier",
            name: "Ana Torres",
            score: 0.42,
            speaker: at("c", "Architecture sync — caching strategy", 9, "speaker_2"),
            compared: [
              { speaker: at(record.id, record.title, 0, "speaker_1"), score: 0.38 },
              { speaker: at("b", "1:1 with Sarah", 1, "speaker_3"), score: 0.45 },
            ],
          },
          { kind: "look_alike", key: "alike", names: ["Marcus Chen", "Sarah Lin"], score: 0.64 },
        ];
      }
      case "sessions_with_speaker":
        return [record];
      case "search":
        return [
          {
            session_id: record.id,
            title: record.title,
            folder: "Work",
            started_at: record.started_at,
            start: 52_000,
            speaker: "Ana Torres",
            snippet: "make sure the [rollback] plan is documented before Friday",
          },
          {
            session_id: "c",
            title: "Architecture sync — caching strategy",
            folder: "Work",
            started_at: iso(9, 9, 0),
            start: 754_000,
            speaker: "Me",
            snippet: "if the cache fails we need a [rollback] path that doesn't touch the database",
          },
        ];
      case "missing_models":
        return view === "setup"
          ? [
              { label: "Whisper large-v3 turbo (transcription)", bytes: 574_041_195 },
              { label: "Whisper base (drafts and languages)", bytes: 147_951_465 },
              { label: "TitaNet (voices)", bytes: 40_257_283 },
              { label: "pyannote segmentation (who spoke when)", bytes: 6_958_444 },
            ]
          : [];
      case "download_models":
        return new Promise(() => {});
      case "detected_meeting":
        if (view === "popup-ended") return { app: "Microsoft Teams", browser: false, ended: true };
        return view === "meeting" || view === "popup"
          ? { app: "Microsoft Teams", browser: false, ended: false }
          : null;
      case "session_languages":
        return ["en", "es"];
      case "retranscribe_session":
        return new Promise(() => {});
      case "set_mic_muted":
        micMuted = Boolean((args as { muted?: boolean } | undefined)?.muted);
        return null;
      case "recording_status":
        return recording
          ? {
              paused: false,
              elapsed_ms: 2_457_000,
              uptime_ms: 2_460_000,
              mic_level: 0.55,
              system_level: 0.75,
              mic_speaking: false,
              system_speaking: true,
              mic_receiving: true,
              system_receiving: true,
              mic_muted: micMuted,
              restarts: view === "live-silent" ? 1 : 0,
              mic_silent: view === "live-silent" && !micMuted,
            }
          : null;
      case "list_models":
        return [
          { id: "qwen3:14b", label: "qwen3:14b" },
          { id: "qwen3-coder:latest", label: "qwen3-coder:latest" },
        ];
      case "api_keys":
        return { anthropic: false, gemini: false };
      default:
        return null;
    }
  });

  const nav = {
    live: { kind: "live" },
    setup: { kind: "live" },
    permissions: { kind: "live" },
    meeting: { kind: "folder", folder: null },
    library: { kind: "folder", folder: null },
    session: { kind: "session", id: record.id },
    retranscribing: { kind: "session", id: record.id },
    search: { kind: "search", query: "rollback" },
    speakers: { kind: "speakers" },
    settings: { kind: "settings" },
  } as const;
  useNav.setState({ view: nav[view as keyof typeof nav] ?? nav.live });
  if (view === "retranscribing") {
    useRetranscribe.setState({
      jobs: {
        [record.id]: { status: "running", message: "Transcribing…", done: 41, total: 96 },
      },
    });
  }

  if (recording) {
    useTranscript.setState({
      title: "Sprint Release Review",
      segments: segments.map((s) => ({ ...s, speaker: null })),
      drafts: [
        {
          channel: "system",
          start: 60_000,
          text: "And one more thing about the staging environment, we need to",
        },
      ],
      userNotes: "check rollback owner\nrelease notes → product",
    });
    useAsk.setState({
      threads: {
        live: [
          {
            id: 1,
            question: "When is the launch?",
            answer: "Friday morning, if the security review finishes on Thursday [00:26, 00:34].",
            status: "done",
          },
        ],
      },
    });
  }
}
