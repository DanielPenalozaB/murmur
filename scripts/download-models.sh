#!/bin/sh
# Downloads Whisper models into the app's data folder (where the app looks for them).
# Default: large-v3-turbo (transcription) and base (language detection for meetings
# that mix languages). Pass model file names to download others.
set -eu
DIR="$HOME/Library/Application Support/dev.murmur.app/models"
mkdir -p "$DIR"
[ $# -gt 0 ] || set -- ggml-large-v3-turbo-q5_0.bin ggml-base.bin
for MODEL in "$@"; do
  if [ -f "$DIR/$MODEL" ]; then echo "already present: $DIR/$MODEL"; continue; fi
  curl -fL --progress-bar -o "$DIR/$MODEL.part" "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL"
  mv "$DIR/$MODEL.part" "$DIR/$MODEL"
  echo "saved $DIR/$MODEL"
done
