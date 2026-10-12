//! Speech-to-text (whisper.cpp via `whisper-rs`, Metal) and post-meeting
//! speaker diarization (sherpa-onnx). Wired up in milestones 2 and 4.

use murmur_audio::SpeechChunk;
use murmur_core::Segment;

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model not found at {0}")]
    ModelMissing(String),
    #[error("transcription failed: {0}")]
    Inference(String),
}

pub trait Transcriber: Send {
    fn transcribe(&mut self, chunk: &SpeechChunk) -> Result<Vec<Segment>, SttError>;
}
