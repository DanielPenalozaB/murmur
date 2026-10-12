# ADR-002: Audio capture via a Swift sidecar

**Status:** accepted · 2026-10-12 (revised: mic moved into the helper too)

## Context
Teams/Meet/Slack audio has to be captured without a bot joining the call or a virtual
driver like BlackHole. macOS 14.4+ offers Core Audio process taps for this.

## Decision
A small Swift executable (`native/audiocap`) captures one source per process:
`--source system` (process tap → private aggregate device) or `--source mic`
(AVAudioEngine). Both resample with AVAudioConverter and write 16 kHz mono f32 PCM to
stdout. Rust spawns one process per channel and reads 100 ms frames. The helper exits
when stdin closes, so a crashed app never leaves a capture running.

The first plan used `cpal` + `rubato` for the mic in Rust. Putting the mic in the helper
gives both channels the same resampler and format, and removes two dependencies.

## Consequences
Swift calls Apple's APIs directly, with no unsafe FFI in Rust. The helper can be
tested on its own (`audiocap --source system > out.pcm`). It needs
`NSAudioCaptureUsageDescription` / `NSMicrophoneUsageDescription` and is bundled as a
Tauri `externalBin` (`scripts/build-audiocap.sh`). Minimum macOS is 14.4.
