# Murmur: agent notes

Local-first macOS meeting transcription. Read `docs/ARCHITECTURE.md` first.

## Memory (engram)

If the [`engram`](https://github.com/Gentleman-Programming/engram) CLI is installed, use it to
carry context between sessions:

- At the start of a task: `engram context murmur`, and `engram search "<topic>" --project murmur`
  for anything specific (audio capture, speakers, CI, releases, roadmap…).
- After finishing work: `engram save "<title>" "<what was learned>" --project murmur --scope project`
  for durable findings (gotchas, decisions, tuning results, what's next). Keep entries short, and
  update the roadmap entry when plans change.

## Notes

- Node 22 (`.nvmrc`), pnpm, stable Rust, Swift 6 (Command Line Tools are enough).
- `pnpm tauri dev` runs the app · `pnpm check` runs tsc + Biome · `pnpm test` runs vitest + cargo test.
- Crates under `crates/` must not depend on Tauri; put pure logic there with unit tests.
- Never send audio off-device. LLM calls go to a cloud provider only when the user picked one.
- API keys live in the macOS Keychain; never pass them to the webview or write them to disk.
- Models: the app downloads them on first run (setup screen); `scripts/download-models.sh` does
  the same from a terminal (Whisper turbo + base, sherpa-onnx speaker models).
- Slow end-to-end check of speakers (needs models + macOS `say`):
  `cargo test --release -p murmur-stt --test speakers_pipeline -- --ignored`.
- UI preview without the native app: with `pnpm dev` running, open
  `http://127.0.0.1:1420/?preview&view=live|live-muted|live-silent|permissions|setup|empty|start|meeting|popup|popup-ended|popup-collapsed|session|retranscribing|library|search|speakers|settings`
  (Tauri IPC is mocked with sample data from `src/dev/preview.ts`; dev builds only).
- Problems (warnings, errors, panics) are kept in `~/Library/Logs/dev.murmur.app/murmur.log`;
  read it first when the user reports a bad recording.
- README screenshots (`docs/screenshots/<view>-light|dark.png`) come from that preview, so
  they only show sample data: 1280×800 at 1.5× scale (the meeting card at 372×56), in both
  color schemes. Never screenshot the real app or real sessions for the repo. Speaker review
  findings only show with `&review` added to the URL.
- Design source: Google Stitch project 15220382042260412057. Tokens live in `src/styles.css`.
