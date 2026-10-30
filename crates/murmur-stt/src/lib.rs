//! Speech-to-text (whisper.cpp via `whisper-rs`, Metal) and post-meeting speaker
//! diarization (sherpa-onnx).

mod filter;
mod language;
pub mod review;
pub mod speakers;
mod whisper;

pub use filter::{foreign_letters, speech_level_db};
pub use language::{LanguageDetector, MeetingTranscriber};
pub use whisper::{WhisperModel, WhisperTranscriber};

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

/// Whisper invents text on silence and noise, mostly phrases from its YouTube
/// training data. Drops those and non-speech tags like `[BLANK_AUDIO]` or `(music)`.
pub fn is_hallucination(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let bracketed = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'));
    if bracketed || t.chars().all(|c| !c.is_alphanumeric()) {
        return true;
    }
    let lower = t.to_lowercase();
    // A lone "you" is Whisper's most common line for near-silence; nobody says just that.
    if lower.trim_end_matches(|c: char| !c.is_alphanumeric()) == "you" {
        return true;
    }
    const KNOWN: [&str; 11] = [
        "thanks for watching",
        "thank you for watching",
        "please subscribe",
        "the next video",
        "the next episode",
        "subtítulos realizados por",
        "subtitulado por",
        "amara.org",
        "antarctica films",
        "gracias por ver el video",
        "suscríbete",
    ];
    KNOWN.iter().any(|k| lower.contains(k))
}

#[cfg(test)]
mod tests {
    use super::is_hallucination;

    #[test]
    fn filters_known_hallucinations_and_tags() {
        for t in [
            "",
            "  ",
            "[BLANK_AUDIO]",
            "(music)",
            "...",
            "Thanks for watching!",
            "Subtítulos realizados por la comunidad de Amara.org",
            "I'm going to go to the next video.",
            "CC por Antarctica Films Argentina",
            "you",
            " You. ",
        ] {
            assert!(is_hallucination(t), "{t:?}");
        }
    }

    #[test]
    fn keeps_real_speech() {
        for t in [
            "Thank you.",
            "See you.",
            "You too.",
            "Let's ship it on Friday.",
            "¿Quién toma la tarea?",
        ] {
            assert!(!is_hallucination(t), "{t:?}");
        }
    }
}
