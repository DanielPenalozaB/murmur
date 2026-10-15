#!/bin/sh
# Builds the Swift capture helper and places it where Tauri's externalBin expects it.
set -eu
cd "$(dirname "$0")/.."
swift build -c release --package-path native/audiocap
triple="$(rustc --print host-tuple)"
mkdir -p src-tauri/binaries
cp native/audiocap/.build/release/audiocap "src-tauri/binaries/audiocap-$triple"
# `tauri dev` copies the helper next to the app binary only when it rebuilds the app;
# refresh that copy too so a running dev app picks up the new helper on its next recording.
if [ -f target/debug/audiocap ]; then
  cp native/audiocap/.build/release/audiocap target/debug/audiocap
fi
