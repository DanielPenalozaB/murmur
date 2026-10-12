//! Cuts a channel's audio into speech chunks for Whisper.
//!
//! A chunk starts at voice onset (with a short pre-roll so the first syllable isn't
//! clipped) and ends after a run of silence. Long monologues are force-cut at the
//! quietest point near `max_chunk_ms`, which keeps latency bounded.

use std::collections::VecDeque;

use murmur_core::Channel;

use crate::{SAMPLE_RATE, SpeechChunk};

/// Earshot's fixed frame size: 16 ms at 16 kHz.
const VAD_FRAME: usize = 256;

pub trait VoiceScorer: Send {
    /// Probability (0..1) that a 256-sample frame contains speech.
    fn score(&mut self, frame: &[f32]) -> f32;
}

impl VoiceScorer for earshot::Detector {
    fn score(&mut self, frame: &[f32]) -> f32 {
        self.predict_f32(frame)
    }
}

#[derive(Debug, Clone)]
pub struct SegmenterConfig {
    pub threshold: f32,
    pub pre_roll_ms: u64,
    pub min_silence_ms: u64,
    /// Silence kept at the end of a chunk; Whisper does better with a little tail.
    pub tail_ms: u64,
    pub min_speech_ms: u64,
    pub max_chunk_ms: u64,
    /// How far back from `max_chunk_ms` to look for a quiet cut point.
    pub cut_window_ms: u64,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        SegmenterConfig {
            threshold: 0.5,
            pre_roll_ms: 300,
            min_silence_ms: 600,
            tail_ms: 200,
            min_speech_ms: 250,
            max_chunk_ms: 15_000,
            cut_window_ms: 3_000,
        }
    }
}

pub struct Segmenter {
    channel: Channel,
    config: SegmenterConfig,
    scorer: Box<dyn VoiceScorer>,
    /// Samples waiting to fill a whole VAD frame.
    pending: Vec<f32>,
    pre_roll: VecDeque<f32>,
    /// The chunk being built, and one score per 256-sample frame in it.
    chunk: Vec<f32>,
    scores: Vec<f32>,
    chunk_start: u64,
    in_speech: bool,
    silence: usize,
    speech: usize,
    /// Session position (in samples) of the next frame.
    position: u64,
}

impl Segmenter {
    pub fn new(channel: Channel) -> Self {
        Self::with_scorer(
            channel,
            SegmenterConfig::default(),
            Box::new(earshot::Detector::default()),
        )
    }

    pub fn with_scorer(channel: Channel, config: SegmenterConfig, scorer: Box<dyn VoiceScorer>) -> Self {
        Segmenter {
            channel,
            config,
            scorer,
            pending: Vec::with_capacity(VAD_FRAME),
            pre_roll: VecDeque::new(),
            chunk: Vec::new(),
            scores: Vec::new(),
            chunk_start: 0,
            in_speech: false,
            silence: 0,
            speech: 0,
            position: 0,
        }
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    /// Feeds samples and returns any chunks that completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<SpeechChunk> {
        let mut done = Vec::new();
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / VAD_FRAME * VAD_FRAME;
        let frames: Vec<f32> = self.pending.drain(..whole).collect();
        for frame in frames.as_chunks::<VAD_FRAME>().0 {
            if let Some(chunk) = self.push_frame(frame) {
                done.push(chunk);
            }
        }
        done
    }

    /// Emits the in-progress chunk, if any (call when recording stops).
    pub fn flush(&mut self) -> Option<SpeechChunk> {
        if !self.in_speech {
            return None;
        }
        self.in_speech = false;
        let samples = std::mem::take(&mut self.chunk);
        self.scores.clear();
        let speech = std::mem::take(&mut self.speech);
        self.emit(self.chunk_start, samples, speech)
    }

    fn push_frame(&mut self, frame: &[f32]) -> Option<SpeechChunk> {
        let is_speech = self.scorer.score(frame) >= self.config.threshold;
        self.position += VAD_FRAME as u64;

        if !self.in_speech {
            self.pre_roll.extend(frame);
            // Whole frames only, so `scores` stays aligned with `chunk`.
            let keep = ms_to_samples(self.config.pre_roll_ms).div_ceil(VAD_FRAME) * VAD_FRAME;
            while self.pre_roll.len() > keep {
                self.pre_roll.pop_front();
            }
            if is_speech {
                self.in_speech = true;
                self.chunk_start = self.position - self.pre_roll.len() as u64;
                self.chunk = self.pre_roll.drain(..).collect();
                self.scores = vec![0.0; self.chunk.len() / VAD_FRAME];
                self.scores.pop();
                self.scores.push(1.0);
                self.silence = 0;
                self.speech = VAD_FRAME;
            }
            return None;
        }

        self.chunk.extend_from_slice(frame);
        self.scores.push(if is_speech { 1.0 } else { 0.0 });
        if is_speech {
            self.silence = 0;
            self.speech += VAD_FRAME;
        } else {
            self.silence += VAD_FRAME;
        }

        if self.silence >= ms_to_samples(self.config.min_silence_ms) {
            self.in_speech = false;
            let mut samples = std::mem::take(&mut self.chunk);
            self.scores.clear();
            let excess = self.silence.saturating_sub(ms_to_samples(self.config.tail_ms));
            samples.truncate(samples.len() - excess);
            let speech = std::mem::take(&mut self.speech);
            return self.emit(self.chunk_start, samples, speech);
        }

        if self.chunk.len() >= ms_to_samples(self.config.max_chunk_ms) {
            return self.force_cut();
        }
        None
    }

    /// Splits a too-long chunk at the quietest frame near its end; speech continues
    /// in a new chunk from that point.
    fn force_cut(&mut self) -> Option<SpeechChunk> {
        let window = ms_to_samples(self.config.cut_window_ms) / VAD_FRAME;
        let first = self.scores.len().saturating_sub(window);
        // Prefer the latest silent frame so the first chunk stays as long as possible.
        let cut_frame = (first..self.scores.len())
            .rev()
            .find(|&i| self.scores[i] == 0.0)
            .unwrap_or(self.scores.len());
        let cut = (cut_frame * VAD_FRAME).min(self.chunk.len());

        let rest = self.chunk.split_off(cut);
        let head = std::mem::replace(&mut self.chunk, rest);
        self.scores.drain(..cut_frame.min(self.scores.len()));
        let head_start = self.chunk_start;
        self.chunk_start += head.len() as u64;
        self.speech = self.chunk.len();
        let head_speech = head.len();
        self.emit(head_start, head, head_speech)
    }

    fn emit(&self, start: u64, samples: Vec<f32>, speech: usize) -> Option<SpeechChunk> {
        if speech < ms_to_samples(self.config.min_speech_ms) || samples.is_empty() {
            return None;
        }
        Some(SpeechChunk {
            channel: self.channel,
            start_ms: start * 1000 / SAMPLE_RATE as u64,
            samples,
        })
    }
}

fn ms_to_samples(ms: u64) -> usize {
    (ms * SAMPLE_RATE as u64 / 1000) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Treats any frame with a non-zero first sample as speech.
    struct Loudness;
    impl VoiceScorer for Loudness {
        fn score(&mut self, frame: &[f32]) -> f32 {
            if frame[0] != 0.0 { 1.0 } else { 0.0 }
        }
    }

    fn segmenter(config: SegmenterConfig) -> Segmenter {
        Segmenter::with_scorer(Channel::System, config, Box::new(Loudness))
    }

    fn audio(ms: u64, speech: bool) -> Vec<f32> {
        vec![if speech { 0.5 } else { 0.0 }; ms_to_samples(ms)]
    }

    #[test]
    fn emits_chunk_after_silence_with_pre_roll_and_tail() {
        let mut s = segmenter(SegmenterConfig::default());
        assert!(s.push(&audio(1_024, false)).is_empty());
        assert!(s.push(&audio(1_024, true)).is_empty());
        let chunks = s.push(&audio(1_024, false));
        assert_eq!(chunks.len(), 1);
        let c = &chunks[0];
        // Starts ~300 ms before the onset at 1024 ms, rounded to whole frames.
        assert!((700..=740).contains(&c.start_ms), "start {}", c.start_ms);
        let ms = c.samples.len() as u64 * 1000 / 16_000;
        // pre-roll 300 + speech 1024 + tail ~200
        assert!((1_500..=1_560).contains(&ms), "length {ms}");
    }

    #[test]
    fn ignores_blips_shorter_than_min_speech() {
        let mut s = segmenter(SegmenterConfig::default());
        s.push(&audio(512, false));
        s.push(&audio(64, true));
        assert!(s.push(&audio(1_024, false)).is_empty());
    }

    #[test]
    fn force_cuts_long_speech_at_a_quiet_frame() {
        let config = SegmenterConfig {
            max_chunk_ms: 2_000,
            cut_window_ms: 1_000,
            ..Default::default()
        };
        let mut s = segmenter(config);
        let mut out = Vec::new();
        out.extend(s.push(&audio(1_200, true)));
        out.extend(s.push(&audio(100, false))); // a short pause, below min_silence
        out.extend(s.push(&audio(1_500, true)));
        assert_eq!(out.len(), 1);
        let first = &out[0];
        let first_ms = first.samples.len() as u64 * 1000 / 16_000;
        assert!((1_200..=1_300).contains(&first_ms), "cut at {first_ms}");
        let rest = s.flush().expect("remaining speech");
        assert_eq!(rest.start_ms, first.start_ms + first_ms);
    }

    #[test]
    fn flush_returns_open_chunk() {
        let mut s = segmenter(SegmenterConfig::default());
        s.push(&audio(800, true));
        assert!(s.in_speech());
        assert!(s.flush().is_some());
        assert!(s.flush().is_none());
    }
}
