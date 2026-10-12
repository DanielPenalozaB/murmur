# Murmur: agent notes

Local-first macOS meeting transcription. Read `docs/ARCHITECTURE.md` first.

- Node 22 (`.nvmrc`), pnpm, stable Rust, Swift 6 (Command Line Tools are enough).
- `pnpm tauri dev` runs the app · `pnpm check` runs tsc + Biome · `pnpm test` runs vitest + cargo test.
- Crates under `crates/` must not depend on Tauri; put pure logic there with unit tests.
- Never send audio off-device. LLM calls go to a cloud provider only when the user picked one.
- API keys live in the macOS Keychain; never pass them to the webview or write them to disk.
