# ADR-001: Tauri 2 with a Rust core

**Status:** accepted · 2026-10-12

## Context
The app needs low-latency audio, on-device ML (whisper.cpp, ONNX), and a rich notes UI.

## Decision
Tauri 2: Rust for audio/STT/LLM/storage, React + TypeScript for the UI.

## Alternatives
- **Electron + Node:** familiar, but ~10x the memory, and the ML still has to be native addons.
- **SwiftUI:** best macOS integration, but a weaker editor/UI ecosystem and no path to Windows.

## Consequences
Learning Rust is part of the project. macOS-only APIs are isolated in the Swift helper.
