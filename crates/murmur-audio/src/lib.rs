//! Audio capture. The mic and system audio stay on separate channels, which
//! gives "Me vs Them" attribution without any diarization model.
//!
//! Both channels come from the `audiocap` Swift helper (`native/audiocap`), which
//! writes 16 kHz mono f32 PCM to stdout. See ADR-002.

mod helper;
mod normalize;
mod recorder;
mod vad;

pub use helper::HelperProcess;
pub use normalize::normalize_wav;
pub use recorder::{Recorder, RecorderStatus, Recording, TrackFile};
pub use vad::{Segmenter, SegmenterConfig, VoiceScorer};

use murmur_core::Channel;

pub const SAMPLE_RATE: u32 = 16_000;

/// A chunk of speech cut by the VAD, ready for transcription.
#[derive(Debug, Clone)]
pub struct SpeechChunk {
    pub channel: Channel,
    /// Offset from session start in milliseconds.
    pub start_ms: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("{channel:?} capture failed: {message}")]
    Helper { channel: Channel, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Wav(#[from] hound::Error),
}
