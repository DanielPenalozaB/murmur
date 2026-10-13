# Murmur: agent notes

Local-first macOS meeting transcription. Read `docs/ARCHITECTURE.md` first.

- Node 22 (`.nvmrc`), pnpm, stable Rust, Swift 6 (Command Line Tools are enough).
- `pnpm tauri dev` runs the app · `pnpm check` runs tsc + Biome · `pnpm test` runs vitest + cargo test.
- Crates under `crates/` must not depend on Tauri; put pure logic there with unit tests.
- Never send audio off-device. LLM calls go to a cloud provider only when the user picked one.
- API keys live in the macOS Keychain; never pass them to the webview or write them to disk.
- Models: `scripts/download-models.sh` (Whisper turbo + base, sherpa-onnx speaker models).
- Slow end-to-end check of speakers (needs models + macOS `say`):
  `cargo test --release -p murmur-stt --test speakers_pipeline -- --ignored`.
- UI preview without the native app: with `pnpm dev` running, open
  `http://127.0.0.1:1420/?preview&view=live|empty|start|session|library|search|speakers|settings`
  (Tauri IPC is mocked with sample data from `src/dev/preview.ts`; dev builds only).
- Design source: Google Stitch project 15220382042260412057. Tokens live in `src/styles.css`.
