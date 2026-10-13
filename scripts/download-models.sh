#!/bin/sh
# Downloads models into the app's data folder (where the app looks for them):
#   - Whisper large-v3-turbo (transcription) and base (drafts, language detection)
#   - sherpa-onnx pyannote segmentation + NeMo TitaNet (who spoke when)
# Pass Whisper model file names to download other Whisper models instead.
set -eu
DIR="$HOME/Library/Application Support/dev.murmur.app/models"
SHERPA="https://github.com/k2-fsa/sherpa-onnx/releases/download"
mkdir -p "$DIR/speakers"

fetch() { # url destination
  if [ -f "$2" ]; then echo "already present: $2"; return; fi
  curl -fL --progress-bar -o "$2.part" "$1"
  mv "$2.part" "$2"
  echo "saved $2"
}

if [ $# -gt 0 ]; then
  for MODEL in "$@"; do fetch "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL" "$DIR/$MODEL"; done
  exit 0
fi

for MODEL in ggml-large-v3-turbo-q5_0.bin ggml-base.bin; do
  fetch "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL" "$DIR/$MODEL"
done

# The release tag really is spelled "recongition".
fetch "$SHERPA/speaker-recongition-models/nemo_en_titanet_small.onnx" "$DIR/speakers/nemo_en_titanet_small.onnx"
SEG="$DIR/speakers/sherpa-onnx-pyannote-segmentation-3-0"
if [ -f "$SEG/model.onnx" ]; then
  echo "already present: $SEG/model.onnx"
else
  fetch "$SHERPA/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2" "$DIR/speakers/seg.tar.bz2"
  tar xjf "$DIR/speakers/seg.tar.bz2" -C "$DIR/speakers"
  rm "$DIR/speakers/seg.tar.bz2"
fi
