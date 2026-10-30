# Murmur architecture

Local-first meeting transcription for macOS (Teams, Meet, Slack huddles). Audio never
leaves the machine; the LLM is local (Ollama) by default, cloud (Claude/Gemini with the
user's own API key) only when the user opts in for that session.

## Data flow

```
┌────────────── macOS ──────────────┐
│ Mic ── audiocap --source mic ─────┤ Swift helper, one
│ System ─ audiocap --source system ┤ process per channel:
│   (Core Audio process tap)        │ 16 kHz mono f32 on stdout
└─────────────────────────┼─────────┘
                          ▼
        murmur-audio: 100 ms frames per channel → WAV tracks + earshot VAD
                          │  SpeechChunk { channel, start_ms, samples }
                          ▼
        murmur-stt: whisper.cpp (whisper-rs, Metal) worker thread
                          │  Segment { start, end, channel, text }
                          ▼
        session actor (src-tauri, tokio) ── append ──► transcript (in memory + SQLite)
            │                    │                              │
            │ tauri::ipc::Channel│ Ask question                 │ Stop
            ▼                    ▼                              ▼
        React UI          murmur-llm: transcript prefix   diarize (sherpa-onnx) → summary
        (live lines)      + question → stream answer      → murmur-store writes .md/.txt
```

Speaker identity has two layers:

1. **Live:** the channel decides it. Mic = "Me", system audio = "Them". Free and reliable.
2. **After stop:** sherpa-onnx speaker embeddings cluster the System channel into
   Speaker 1..N. The user renames them, and names are remembered as voiceprints later.

## Repo layout

```
murmur/
├── Cargo.toml              Cargo workspace (shared deps + release profile)
├── src/                    React 19 + TypeScript UI (Vite, Tailwind v4)
├── src-tauri/              Tauri 2 shell: commands, session actor, app state
├── crates/
│   ├── murmur-core/        domain types only (Session, Segment, Speaker…), no I/O
│   ├── murmur-audio/       capture, resampling, VAD chunking
│   ├── murmur-stt/         Transcriber trait, whisper + diarization
│   ├── murmur-llm/         Provider enum, prompt builders, streaming clients
│   └── murmur-store/       Markdown/TXT export, SQLite index (FTS5)
├── native/audiocap/        Swift package: mic + system audio capture helper (sidecar)
├── models/                 downloaded models (git-ignored)
└── docs/adr/               architecture decision records
```

Rule: crates depend on `murmur-core` and nothing above them; only `src-tauri` knows about
Tauri. That keeps the hard parts (audio, STT, prompts) unit-testable with `cargo test`.

## Libraries

| Concern | Choice |
|---|---|
| App shell | Tauri 2 |
| UI | React 19, TypeScript, Vite, Tailwind v4, TanStack Query (server state), Zustand (UI state) |
| Notes | Markdown textarea + `react-markdown`/`remark-gfm` rendering (a rich editor can come later) |
| Typed IPC | `tauri-specta` to generate TS bindings from Rust commands (milestone 2) |
| Async runtime | tokio |
| Mic capture | Swift helper, AVAudioEngine |
| System audio | Swift helper, Core Audio process taps (macOS 14.4+) |
| Resampling | AVAudioConverter (in the helper) |
| WAV files | `hound` |
| VAD | `earshot` (pure Rust, no ONNX runtime); chunks cut after 600 ms silence, max 15 s |
| STT | `whisper-rs` (whisper.cpp, Metal); default model large-v3-turbo q5; `ggml-base` for language ID in mixed-language meetings |
| Diarization | official `sherpa-onnx` crate: pyannote segmentation 3.0 + NeMo TitaNet small embeddings (static link) |
| UI type & icons | Newsreader + Hanken Grotesk via `@fontsource-variable` (bundled, no network), `lucide-react` |
| HTTP / streaming | `reqwest` + SSE parsing for Ollama, Anthropic, Gemini |
| Secrets | `security-framework` (macOS Keychain); API keys never reach the webview |
| Database | `rusqlite` (bundled, FTS5) |
| Logging | `tracing` |

## Key decisions

- [ADR-001](adr/001-tauri-rust-core.md): Tauri + Rust core rather than Electron or SwiftUI
- [ADR-002](adr/002-system-audio-swift-helper.md): system audio via a Swift sidecar
- [ADR-003](adr/003-files-are-source-of-truth.md): Markdown/TXT files are the source of truth, SQLite is an index
- [ADR-004](adr/004-ask-prompt-layout.md): Ask uses an append-only transcript prefix
- [ADR-005](adr/005-mixed-language-meetings.md): meetings that mix languages are split at language switches

## Milestones

1. **Capture:** mic + system audio → two WAV files, with start/pause/stop. ✅ (in-app permissions screen still to do)
2. **Live transcript:** VAD → whisper → segments streamed to the UI. ✅ (typed IPC via tauri-specta deferred)
3. **Library:** folders, sessions, SQLite index (FTS5), Markdown/TXT export. ✅ (notes editor moves to milestone 4 with AI notes; re-indexing files edited outside the app still to do)
   - Mixed-language meetings (English ↔ Spanish switches) ✅
4. **Intelligence:** Ask panel (streaming answers with timestamp citations), post-meeting summary, provider settings + Keychain. ✅
   - Ask works live and on saved sessions ("/btw" prefix accepted, ⌘K to focus); citations jump to the transcript.
   - Notes (summary, decisions, action items, open questions) are written when a recording stops, from the transcript plus the user's own notes, which are kept verbatim under "My notes".
   - Notes live in the session file between `<!-- murmur:notes -->` markers, so regenerating or editing them never touches the transcript or edits made outside those markers.
5. **Speakers:** post-meeting diarization, rename speakers, voiceprints. ✅
   - After Stop, the system-audio track is diarized; speakers are numbered by first appearance and voices matching a remembered one (cosine ≥ 0.6) are named automatically.
   - Renaming rewrites the transcript section of the file, replaces the old name in the notes, and remembers the voice (averaged voiceprint per name).
   - Live drafts: the base model transcribes the utterance in progress every 1.5 s; the final turbo line replaces it.
   - UI rebuilt from the Google Stitch screens (live session, post-meeting notes, library, search, speaker directory, preferences, new session sheet).
6. **Polish:** meeting auto-detect, menu-bar recorder, signing + notarization, auto-update.
   - Library management: rename, move and delete sessions (the session page's ⋯ menu) and folders (the folder page). The vault file moves first, with its header updated, and the index follows; a file is never overwritten. Folders are deleted only when they hold no sessions and none of the user's own files.
   - Deleting a session's audio keeps the transcript and notes; such a session can't be re-transcribed. Settings shows how much space recordings take.
