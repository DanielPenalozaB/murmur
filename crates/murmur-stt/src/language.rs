//! Transcription for meetings that mix languages.
//!
//! Whisper decodes a chunk in one language and translates whatever is said in another,
//! so a chunk that switches from English to Spanish half way comes out all English. A
//! small model (base, ~50 ms per check) identifies the language on 3 s windows,
//! restricted to the meeting's languages. Chunks are cut at the quietest point near
//! each switch and each part is transcribed with its language forced.

use std::sync::Arc;

use murmur_audio::{SAMPLE_RATE, SpeechChunk};
use murmur_core::Segment;
use whisper_rs::WhisperState;

use crate::{SttError, Transcriber, WhisperModel, WhisperTranscriber};

const RATE: usize = SAMPLE_RATE as usize;
/// Language ID window and step.
const WINDOW: usize = 3 * RATE;
const HOP: usize = RATE;
/// A language must hold for this many consecutive windows (about 2 s of speech) to
/// count as a switch. Shorter runs, like a borrowed word, stay in the surrounding
/// language; Whisper handles those fine.
const MIN_RUN: usize = 2;
/// How far around an estimated switch to look for a quiet cut point.
const CUT_SEARCH: usize = RATE;
/// Loudness is compared over 20 ms frames when choosing a cut.
const CUT_FRAME: usize = RATE / 50;

/// Identifies which of the meeting's languages a stretch of audio is in.
pub struct LanguageDetector {
    _model: Arc<WhisperModel>,
    state: WhisperState,
    /// ISO code and Whisper language id, in the meeting's order.
    languages: Vec<(String, usize)>,
}

impl LanguageDetector {
    /// `languages` are ISO codes like "en" and "es"; at least two.
    pub fn new(model: &Arc<WhisperModel>, languages: &[String]) -> Result<Self, SttError> {
        if languages.len() < 2 {
            return Err(SttError::Inference(
                "language detection needs two or more languages".into(),
            ));
        }
        let languages = languages
            .iter()
            .map(|code| {
                whisper_rs::get_lang_id(code)
                    .map(|id| (code.clone(), id as usize))
                    .ok_or_else(|| SttError::Inference(format!("unknown language {code:?}")))
            })
            .collect::<Result<_, _>>()?;
        let state = model
            .ctx
            .create_state()
            .map_err(|e| SttError::Inference(e.to_string()))?;
        Ok(LanguageDetector {
            _model: model.clone(),
            state,
            languages,
        })
    }

    /// Index into the meeting's languages of the most likely one.
    pub fn detect(&mut self, samples: &[f32]) -> Result<usize, SttError> {
        let probs = self.probabilities(samples)?;
        Ok(argmax(&probs))
    }

    /// Probability of each meeting language, summing to 1.
    pub fn probabilities(&mut self, samples: &[f32]) -> Result<Vec<f32>, SttError> {
        let err = |e: whisper_rs::WhisperError| SttError::Inference(format!("language id: {e}"));
        // Like transcription, very short input needs padding to at least 1 s.
        let min = RATE + RATE / 10;
        if samples.len() < min {
            let mut padded = samples.to_vec();
            padded.resize(min, 0.0);
            self.state.pcm_to_mel(&padded, 4).map_err(err)?;
        } else {
            self.state.pcm_to_mel(samples, 4).map_err(err)?;
        }
        let (_, all) = self.state.lang_detect(0, 4).map_err(err)?;
        let probs: Vec<f32> = self.languages.iter().map(|&(_, id)| all[id]).collect();
        let total: f32 = probs.iter().sum();
        if total <= 0.0 {
            return Ok(vec![1.0 / probs.len() as f32; probs.len()]);
        }
        Ok(probs.into_iter().map(|p| p / total).collect())
    }

    /// Splits a chunk where its language changes. Most chunks are in one language:
    /// three probes (start, middle, end) agree and the chunk comes back whole.
    pub fn split(&mut self, chunk: &SpeechChunk) -> Result<Vec<(SpeechChunk, String)>, SttError> {
        let samples = &chunk.samples;
        let n = samples.len();
        let whole = |this: &Self, lang: usize| vec![(chunk.clone(), this.languages[lang].0.clone())];

        if n <= WINDOW + HOP {
            let lang = self.detect(samples)?;
            return Ok(whole(self, lang));
        }
        let mid = n / 2 - WINDOW / 2;
        let probes = [
            self.detect(&samples[..WINDOW])?,
            self.detect(&samples[mid..mid + WINDOW])?,
            self.detect(&samples[n - WINDOW..])?,
        ];
        if probes.iter().all(|&l| l == probes[0]) {
            return Ok(whole(self, probes[0]));
        }

        // The probes disagree: scan the chunk second by second.
        let mut starts: Vec<usize> = (0..=n - WINDOW).step_by(HOP).collect();
        if starts.last() != Some(&(n - WINDOW)) {
            starts.push(n - WINDOW);
        }
        let mut labels = Vec::with_capacity(starts.len());
        for &s in &starts {
            labels.push(self.detect(&samples[s..s + WINDOW])?);
        }
        let runs = language_runs(&labels, MIN_RUN);

        let mut parts = Vec::with_capacity(runs.len());
        let mut from = 0;
        for pair in runs.windows(2) {
            // The switch lies between the centers of the last window of one run and
            // the first window of the next.
            let (last, first) = (starts[pair[0].end - 1], starts[pair[1].start]);
            let estimate = (last + first) / 2 + WINDOW / 2;
            let cut = quietest_point(
                samples,
                estimate.saturating_sub(CUT_SEARCH),
                estimate + CUT_SEARCH,
            );
            if cut > from {
                parts.push((from, cut, pair[0].label));
                from = cut;
            }
        }
        parts.push((from, n, runs.last().expect("at least one run").label));

        Ok(parts
            .into_iter()
            .map(|(start, end, lang)| {
                let part = SpeechChunk {
                    channel: chunk.channel,
                    start_ms: chunk.start_ms + (start * 1000 / RATE) as u64,
                    samples: samples[start..end].to_vec(),
                };
                (part, self.languages[lang].0.clone())
            })
            .collect())
    }
}

/// Transcribes a meeting in its languages: none means Whisper auto-detects per chunk,
/// one is forced for every chunk, and two or more turn on switch detection.
pub struct MeetingTranscriber {
    whisper: WhisperTranscriber,
    language: Option<String>,
    detector: Option<LanguageDetector>,
}

impl MeetingTranscriber {
    /// `detector_model` is required when `languages` has two or more entries.
    pub fn new(
        model: &Arc<WhisperModel>,
        detector_model: Option<&Arc<WhisperModel>>,
        languages: &[String],
    ) -> Result<Self, SttError> {
        let whisper = model.transcriber(None)?;
        if languages.len() < 2 {
            return Ok(MeetingTranscriber {
                whisper,
                language: languages.first().cloned(),
                detector: None,
            });
        }
        let detector_model = detector_model
            .ok_or_else(|| SttError::Inference("a language detection model is required".into()))?;
        Ok(MeetingTranscriber {
            whisper,
            language: None,
            detector: Some(LanguageDetector::new(detector_model, languages)?),
        })
    }
}

impl Transcriber for MeetingTranscriber {
    fn transcribe(&mut self, chunk: &SpeechChunk) -> Result<Vec<Segment>, SttError> {
        let Some(detector) = &mut self.detector else {
            return self.whisper.transcribe_in(chunk, self.language.as_deref());
        };
        let mut segments = Vec::new();
        for (part, language) in detector.split(chunk)? {
            segments.extend(self.whisper.transcribe_in(&part, Some(&language))?);
        }
        Ok(segments)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    label: usize,
    /// Window indices, end exclusive.
    start: usize,
    end: usize,
}

/// Groups per-window language labels into runs, folding runs shorter than `min_run`
/// windows into a neighbor so a single uncertain window doesn't cause a split.
fn language_runs(labels: &[usize], min_run: usize) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (i, &label) in labels.iter().enumerate() {
        match runs.last_mut() {
            Some(run) if run.label == label => run.end = i + 1,
            _ => runs.push(Run {
                label,
                start: i,
                end: i + 1,
            }),
        }
    }
    // Repeatedly absorb the shortest too-short run into its longer neighbor.
    while runs.len() > 1 {
        let Some((i, _)) = runs
            .iter()
            .enumerate()
            .filter(|(_, r)| r.end - r.start < min_run)
            .min_by_key(|(_, r)| r.end - r.start)
        else {
            break;
        };
        let len = |r: &Run| r.end - r.start;
        let into_prev = match (i.checked_sub(1).map(|p| &runs[p]), runs.get(i + 1)) {
            (Some(prev), Some(next)) => len(prev) >= len(next),
            (Some(_), None) => true,
            _ => false,
        };
        let short = runs.remove(i);
        if into_prev {
            runs[i - 1].end = short.end;
        } else {
            runs[i].start = short.start;
        }
        // Absorbing may leave two neighbors with the same language side by side.
        let mut merged: Vec<Run> = Vec::with_capacity(runs.len());
        for run in runs {
            match merged.last_mut() {
                Some(last) if last.label == run.label => last.end = run.end,
                _ => merged.push(run),
            }
        }
        runs = merged;
    }
    runs
}

/// Start of the quietest 20 ms frame between `from` and `to`, so a cut falls in a pause
/// between words rather than inside one.
fn quietest_point(samples: &[f32], from: usize, to: usize) -> usize {
    let to = to.min(samples.len());
    let mut best = (f32::INFINITY, from.min(to));
    let mut start = from;
    while start + CUT_FRAME <= to {
        let energy: f32 = samples[start..start + CUT_FRAME].iter().map(|s| s * s).sum();
        if energy < best.0 {
            best = (energy, start);
        }
        start += CUT_FRAME / 2;
    }
    best.1
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(labels: &[usize]) -> Vec<(usize, usize, usize)> {
        language_runs(labels, MIN_RUN)
            .into_iter()
            .map(|r| (r.label, r.start, r.end))
            .collect()
    }

    #[test]
    fn single_language_is_one_run() {
        assert_eq!(runs(&[0, 0, 0, 0]), [(0, 0, 4)]);
    }

    #[test]
    fn finds_a_switch() {
        assert_eq!(runs(&[0, 0, 0, 1, 1, 1]), [(0, 0, 3), (1, 3, 6)]);
    }

    #[test]
    fn ignores_a_single_window_blip() {
        assert_eq!(runs(&[0, 0, 1, 0, 0]), [(0, 0, 5)]);
        // An uncertain window right at the switch joins the longer side.
        assert_eq!(runs(&[0, 0, 0, 1, 0, 1, 1]), [(0, 0, 5), (1, 5, 7)]);
    }

    #[test]
    fn short_edge_runs_join_their_neighbor() {
        assert_eq!(runs(&[1, 0, 0, 0]), [(0, 0, 4)]);
        assert_eq!(runs(&[0, 0, 0, 1]), [(0, 0, 4)]);
    }

    #[test]
    fn cuts_at_the_quietest_frame() {
        let mut samples = vec![0.5; RATE];
        samples[8_000..8_400].fill(0.0);
        let cut = quietest_point(&samples, 0, RATE);
        assert!((8_000..=8_400 - CUT_FRAME).contains(&cut), "cut at {cut}");
    }
}
