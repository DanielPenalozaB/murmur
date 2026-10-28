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

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md); please report
security or privacy problems privately as described in [SECURITY.md](SECURITY.md).

## License

Murmur is source-available under the [PolyForm Shield License 1.0.0](LICENSE.md),
copyright Daniel Penaloza. You may use, modify and share it, including at work, and fork
it to extend it, as long as you keep the `Required Notice` (crediting the author and
<https://github.com/DanielPenalozaB/murmur>) and the license with every copy. You may not
sell Murmur or a version of it, or offer it as a product or service that competes with it.
