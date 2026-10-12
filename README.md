# Murmur

Local-first meeting transcription and notes for macOS: Teams, Google Meet and Slack.
Transcription runs on-device with Whisper; summaries and live Q&A use local Qwen via
Ollama, or your own Claude/Gemini API key.

## Setup

```sh
nvm use                  # Node 22
corepack enable          # pnpm
pnpm install
pnpm tauri dev
```

Requires macOS 14.4+, Rust (rustup), Xcode Command Line Tools, cmake, and Ollama.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
