#!/bin/sh
# Builds the Swift capture helper and places it where Tauri's externalBin expects it.
set -eu
cd "$(dirname "$0")/.."
swift build -c release --package-path native/audiocap
triple="$(rustc --print host-tuple)"
mkdir -p src-tauri/binaries
cp native/audiocap/.build/release/audiocap "src-tauri/binaries/audiocap-$triple"
