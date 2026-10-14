//! Catches what Whisper invents rather than hears.
//!
//! Whisper turns faint noise (keyboard, breathing, a muted mic) into stock phrases
//! like "Thank you." and repeats them all meeting. Two checks per channel catch most
//! of it: chunks far quieter than that channel's speech are skipped before
//! transcription, and short lines that repeat word for word are dropped.

use std::collections::VecDeque;

use murmur_audio::{SAMPLE_RATE, SpeechChunk};
use murmur_core::Segment;

/// Loudness is measured over 20 ms frames.
const FRAME: usize = SAMPLE_RATE as usize / 50;
/// Chunks quieter than this (dBFS) are never speech worth transcribing. Speech on a
/// laptop or headset mic sits around −40 to −20 dBFS.
const FLOOR_DB: f32 = -55.0;
/// A chunk this far below the channel's speech level is background, not a speaker.
const BELOW_SPEECH_DB: f32 = 22.0;
/// How fast the channel's speech level relaxes after a loud chunk, per chunk.
const LEVEL_DECAY_DB: f32 = 0.3;
/// Music and steady noise come back as a stray phrase for many seconds of sound,
/// where speech runs at two to three words per second.
const SPARSE_MIN_SECONDS: f32 = 5.0;
const SPARSE_WORDS_PER_SECOND: f32 = 0.8;
/// Lines this short that repeat are treated as invented.
const MAX_REPEAT_WORDS: usize = 6;
/// How far back a repeat is looked for.
const REPEAT_WINDOW_MS: u64 = 120_000;

#[derive(Default)]
pub struct ChannelFilter {
    /// Typical loudness of this channel's speech, once some was heard.
    speech_db: Option<f32>,
    /// Recent short lines (start ms, normalized text).
    recent: VecDeque<(u64, String)>,
}

impl ChannelFilter {
    /// Whether `chunk` is too quiet to be anyone talking. Updates the channel's level.
    pub fn too_quiet(&mut self, chunk: &SpeechChunk) -> bool {
        let level = speech_level_db(&chunk.samples);
        let quiet = level < FLOOR_DB || self.speech_db.is_some_and(|s| level < s - BELOW_SPEECH_DB);
        if !quiet {
            self.speech_db = Some(match self.speech_db {
                Some(s) if s - LEVEL_DECAY_DB > level => s - LEVEL_DECAY_DB,
                _ => level,
            });
        }
        quiet
    }

    /// Whether `segment` repeats a short line said shortly before. Drafts of the same
    /// utterance start at the same time and don't count as repeats.
    pub fn repeated(&mut self, segment: &Segment) -> bool {
        let text = normalize(&segment.text);
        if text.split(' ').count() > MAX_REPEAT_WORDS {
            return false;
        }
        while self
            .recent
            .front()
            .is_some_and(|(at, _)| at + REPEAT_WINDOW_MS < segment.start)
        {
            self.recent.pop_front();
        }
        let repeat = self
            .recent
            .iter()
            .any(|(at, t)| *t == text && *at != segment.start);
        self.recent.push_back((segment.start, text));
        repeat
    }
}

/// Whether the text Whisper produced for `chunk` is far too little for its length,
/// which happens on music and other sound that isn't speech.
pub fn too_sparse(chunk: &SpeechChunk, segments: &[Segment]) -> bool {
    let seconds = chunk.samples.len() as f32 / SAMPLE_RATE as f32;
    let words: usize = segments.iter().map(|s| s.text.split_whitespace().count()).sum();
    seconds >= SPARSE_MIN_SECONDS && (words as f32) < SPARSE_WORDS_PER_SECOND * seconds
}

/// Loudness of the louder frames (90th percentile), in dBFS: how loud the speech in
/// a chunk is, ignoring the pauses around it.
pub fn speech_level_db(samples: &[f32]) -> f32 {
    let mut frames: Vec<f32> = samples
        .chunks(FRAME)
        .map(|f| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt())
        .collect();
    if frames.is_empty() {
        return f32::NEG_INFINITY;
    }
    frames.sort_by(f32::total_cmp);
    let rms = frames[(frames.len() * 9 / 10).min(frames.len() - 1)];
    20.0 * rms.max(1e-9).log10()
}

/// Lowercase words without punctuation, so "Thank you." and "thank you!" match.
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use murmur_core::Channel;

    fn chunk(amplitude: f32) -> SpeechChunk {
        SpeechChunk {
            channel: Channel::Mic,
            start_ms: 0,
            samples: (0..SAMPLE_RATE as usize)
                .map(|i| amplitude * (i as f32 * 0.3).sin())
                .collect(),
        }
    }

    fn seg(start: u64, text: &str) -> Segment {
        Segment {
            start,
            end: start + 1_000,
            channel: Channel::Mic,
            speaker: None,
            text: text.into(),
        }
    }

    #[test]
    fn skips_faint_noise_and_audio_far_below_speech() {
        let mut f = ChannelFilter::default();
        assert!(f.too_quiet(&chunk(0.001)), "−60 dBFS is below the floor");
        assert!(!f.too_quiet(&chunk(0.1)), "−20 dBFS speech");
        assert!(f.too_quiet(&chunk(0.005)), "−46 dBFS is far below that speech");
        assert!(!f.too_quiet(&chunk(0.05)), "a softer speaker still counts");
    }

    #[test]
    fn drops_short_lines_that_repeat() {
        let mut f = ChannelFilter::default();
        assert!(!f.repeated(&seg(1_000, "Thank you.")));
        assert!(!f.repeated(&seg(1_000, "Thank you.")), "a draft of the same line");
        assert!(f.repeated(&seg(9_000, "thank you!")));
        assert!(!f.repeated(&seg(300_000, "Thank you.")), "long after");
        let long = "We agreed to ship the release on Friday afternoon.";
        assert!(!f.repeated(&seg(301_000, long)));
        assert!(!f.repeated(&seg(302_000, long)), "long lines may repeat");
    }

    #[test]
    fn a_stray_phrase_over_long_audio_is_not_speech() {
        let long = SpeechChunk {
            channel: Channel::System,
            start_ms: 0,
            samples: vec![0.1; 14 * SAMPLE_RATE as usize],
        };
        assert!(too_sparse(&long, &[seg(0, "I'm going to go to the next video.")]));
        let speech = "Bueno, tenía una duda sobre el informe, sobre la gráfica de ventas del \
                      trimestre. Actualmente esa gráfica está llegando sin el parámetro";
        assert!(!too_sparse(&long, &[seg(0, speech)]));
        let short = SpeechChunk {
            samples: vec![0.1; 2 * SAMPLE_RATE as usize],
            ..long
        };
        assert!(!too_sparse(&short, &[seg(0, "Sí.")]), "short replies are fine");
    }

    #[test]
    fn speech_level_ignores_pauses() {
        let mut samples = vec![0.0; SAMPLE_RATE as usize];
        samples.extend(chunk(0.1).samples);
        assert!(
            (speech_level_db(&samples) + 23.0).abs() < 1.0,
            "{}",
            speech_level_db(&samples)
        );
    }
}
