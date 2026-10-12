#!/bin/sh
# Downloads the Whisper model into the app's data folder (where the app looks for it).
set -eu
MODEL="${1:-ggml-large-v3-turbo-q5_0.bin}"
DIR="$HOME/Library/Application Support/dev.murmur.app/models"
mkdir -p "$DIR"
if [ -f "$DIR/$MODEL" ]; then echo "already present: $DIR/$MODEL"; exit 0; fi
curl -fL --progress-bar -o "$DIR/$MODEL.part" "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL"
mv "$DIR/$MODEL.part" "$DIR/$MODEL"
echo "saved $DIR/$MODEL"
