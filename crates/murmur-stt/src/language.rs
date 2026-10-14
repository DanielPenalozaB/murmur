//! Transcription for meetings that mix languages.
//!
//! Whisper decodes a chunk in one language and translates whatever is said in another,
//! so a chunk that switches from English to Spanish half way comes out all English. A
//! small model (base, ~50 ms per check) identifies the language on 3 s windows,
//! restricted to the meeting's languages. Chunks are cut at the quietest point near
//! each switch and each part is transcribed with its language forced.
//!
//! Short utterances ("sí", "ok", a laugh) carry little evidence, and noise looks like
//! English to the detector. Each channel therefore keeps a prior over the languages it
//! has been speaking, and a chunk only switches away from it on clear evidence.

use std::sync::Arc;

use murmur_audio::{SAMPLE_RATE, SpeechChunk};
use murmur_core::{Channel, Segment};
use whisper_rs::WhisperState;

use crate::filter::{ChannelFilter, foreign_letters, too_sparse};
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
/// Speech that counts as full evidence of its language; shorter audio counts less.
const FULL_EVIDENCE: f32 = 3.0;
const MIN_EVIDENCE: f32 = 0.3;
/// No language's prior drops below this, so a real switch can still win.
const MIN_PRIOR: f32 = 0.1;
/// Seconds of speech after which the prior has mostly moved to a new language.
const PRIOR_MEMORY: f32 = 10.0;

/// The fast model's answer counts as sure above this probability.
const CONFIDENT: f32 = 0.9;

/// Identifies which of the meeting's languages a stretch of audio is in.
pub struct LanguageDetector {
    _model: Arc<WhisperModel>,
    state: WhisperState,
    /// A larger, slower model asked only when the fast one is unsure or finds a switch.
    confirm: Option<(Arc<WhisperModel>, WhisperState)>,
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
        Ok(LanguageDetector {
            _model: model.clone(),
            state: new_state(model)?,
            confirm: None,
            languages,
        })
    }

    /// Double-checks uncertain answers and switches with `model` (~0.7 s per check
    /// for large-v3-turbo, against ~0.1 s for base).
    pub fn with_confirmation(mut self, model: &Arc<WhisperModel>) -> Result<Self, SttError> {
        self.confirm = Some((model.clone(), new_state(model)?));
        Ok(self)
    }

    /// Index into the meeting's languages of the most likely one.
    pub fn detect(&mut self, samples: &[f32]) -> Result<usize, SttError> {
        let probs = self.probabilities(samples)?;
        Ok(argmax(&probs))
    }

    /// Probability of each meeting language, summing to 1, from the fast model.
    pub fn probabilities(&mut self, samples: &[f32]) -> Result<Vec<f32>, SttError> {
        identify(&mut self.state, &self.languages, samples)
    }

    /// The language of a stretch believed to be in one language, weighed against the
    /// channel's `prior`. `fast` holds the fast model's probabilities for it, when
    /// already known. Asks the confirming model when the fast answer is unsure or
    /// goes against the prior.
    fn decide_stretch(
        &mut self,
        samples: &[f32],
        prior: &[f32],
        fast: Option<Vec<f32>>,
    ) -> Result<usize, SttError> {
        let seconds = samples.len() as f32 / RATE as f32;
        let probs = match fast {
            Some(probs) => probs,
            None => self.probabilities(samples)?,
        };
        let mut choice = decide(&probs, prior, seconds);
        let unsure = probs[choice] < CONFIDENT || choice != argmax(prior);
        let mut confirmed = None;
        if unsure && let Some((_, state)) = &mut self.confirm {
            // Language ID looks at the first 30 s at most.
            let head = &samples[..samples.len().min(30 * RATE)];
            let better = identify(state, &self.languages, head)?;
            choice = decide(&better, prior, seconds);
            confirmed = Some(better);
        }
        if std::env::var_os("MURMUR_STT_TRACE").is_some() {
            eprintln!(
                "lang {seconds:.1}s fast={probs:.2?} confirmed={confirmed:.2?} prior={prior:.2?} -> {}",
                self.languages[choice].0
            );
        }
        Ok(choice)
    }

    /// Splits a chunk where its language changes. Most chunks are in one language:
    /// three probes (start, middle, end) agree and the chunk comes back whole.
    pub fn split(&mut self, chunk: &SpeechChunk) -> Result<Vec<(SpeechChunk, String)>, SttError> {
        let uniform = vec![1.0 / self.languages.len() as f32; self.languages.len()];
        Ok(self
            .split_with(chunk, &uniform)?
            .into_iter()
            .map(|(part, lang)| (part, self.languages[lang].0.clone()))
            .collect())
    }

    /// [`split`](Self::split) with chunks in one language weighed against the
    /// channel's `prior`; returns indices into the meeting's languages.
    pub fn split_with(
        &mut self,
        chunk: &SpeechChunk,
        prior: &[f32],
    ) -> Result<Vec<(SpeechChunk, usize)>, SttError> {
        let samples = &chunk.samples;
        let n = samples.len();
        let whole = |lang: usize| vec![(chunk.clone(), lang)];

        if n <= WINDOW + HOP {
            let lang = self.decide_stretch(samples, prior, None)?;
            return Ok(whole(lang));
        }
        let mid = n / 2 - WINDOW / 2;
        let probes = [
            self.probabilities(&samples[..WINDOW])?,
            self.probabilities(&samples[mid..mid + WINDOW])?,
            self.probabilities(&samples[n - WINDOW..])?,
        ];
        let labels: Vec<usize> = probes.iter().map(|p| argmax(p)).collect();
        if labels.iter().all(|&l| l == labels[0]) {
            // The least sure probe stands for the chunk.
            let weakest = probes
                .into_iter()
                .min_by(|a, b| a[labels[0]].total_cmp(&b[labels[0]]))
                .expect("three probes");
            let lang = self.decide_stretch(samples, prior, Some(weakest))?;
            return Ok(whole(lang));
        }

        // The probes disagree: scan the chunk second by second. Each window is judged
        // on its own; a prior from before the chunk would hide a switch inside it.
        let mut starts: Vec<usize> = (0..=n - WINDOW).step_by(HOP).collect();
        if starts.last() != Some(&(n - WINDOW)) {
            starts.push(n - WINDOW);
        }
        let mut labels = Vec::with_capacity(starts.len());
        for &s in &starts {
            labels.push(self.detect(&samples[s..s + WINDOW])?);
        }
        let mut runs = language_runs(&labels, MIN_RUN);
        // The fast model mistakes accented speech for the other language now and then;
        // the confirming model rechecks each run as a whole. Its answers are sharp
        // enough that a real switch still beats the prior.
        if runs.len() > 1
            && let Some((_, state)) = &mut self.confirm
        {
            for run in &mut runs {
                let (from, to) = (starts[run.start], starts[run.end - 1] + WINDOW);
                let probs = identify(state, &self.languages, &samples[from..to])?;
                run.label = decide(&probs, prior, (to - from) as f32 / RATE as f32);
                if std::env::var_os("MURMUR_STT_TRACE").is_some() {
                    eprintln!(
                        "lang run {from}..{to} confirmed={probs:.2?} -> {}",
                        self.languages[run.label].0
                    );
                }
            }
            runs = merge_runs(runs);
        }

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
                (part, lang)
            })
            .collect())
    }
}

fn new_state(model: &Arc<WhisperModel>) -> Result<WhisperState, SttError> {
    model
        .ctx
        .create_state()
        .map_err(|e| SttError::Inference(e.to_string()))
}

/// Probability of each of `languages` for `samples`, summing to 1.
fn identify(
    state: &mut WhisperState,
    languages: &[(String, usize)],
    samples: &[f32],
) -> Result<Vec<f32>, SttError> {
    let err = |e: whisper_rs::WhisperError| SttError::Inference(format!("language id: {e}"));
    // Like transcription, very short input needs padding to at least 1 s.
    let min = RATE + RATE / 10;
    if samples.len() < min {
        let mut padded = samples.to_vec();
        padded.resize(min, 0.0);
        state.pcm_to_mel(&padded, 4).map_err(err)?;
    } else {
        state.pcm_to_mel(samples, 4).map_err(err)?;
    }
    let (_, all) = state.lang_detect(0, 4).map_err(err)?;
    let probs: Vec<f32> = languages.iter().map(|&(_, id)| all[id]).collect();
    let total: f32 = probs.iter().sum();
    if total <= 0.0 {
        return Ok(vec![1.0 / probs.len() as f32; probs.len()]);
    }
    Ok(probs.into_iter().map(|p| p / total).collect())
}

/// Picks a language from the detector's `probs` for `seconds` of audio and the
/// channel's `prior`: argmax of `evidence · ln p + ln prior`, where evidence grows
/// with the audio's length up to [`FULL_EVIDENCE`] seconds.
fn decide(probs: &[f32], prior: &[f32], seconds: f32) -> usize {
    let evidence = (seconds / FULL_EVIDENCE).clamp(MIN_EVIDENCE, 1.0);
    let scores: Vec<f32> = probs
        .iter()
        .zip(prior)
        .map(|(p, q)| evidence * p.max(1e-6).ln() + q.max(1e-6).ln())
        .collect();
    argmax(&scores)
}

/// Moves `prior` toward `language` in proportion to `seconds` of speech in it.
fn update_prior(prior: &mut [f32], language: usize, seconds: f32) {
    let rate = (seconds / PRIOR_MEMORY).min(0.6);
    for (i, p) in prior.iter_mut().enumerate() {
        let target = if i == language { 1.0 } else { 0.0 };
        *p += rate * (target - *p);
    }
    // Keep every language reachable.
    let floor = MIN_PRIOR.min(1.0 / prior.len() as f32);
    prior.iter_mut().for_each(|p| *p = p.max(floor));
    let total: f32 = prior.iter().sum();
    prior.iter_mut().for_each(|p| *p /= total);
}

/// Transcribes a meeting in its languages: none means Whisper auto-detects per chunk,
/// one is forced for every chunk, and two or more turn on switch detection.
pub struct MeetingTranscriber {
    whisper: WhisperTranscriber,
    language: Option<String>,
    /// The meeting's languages; empty when Whisper picks.
    languages: Vec<String>,
    detector: Option<LanguageDetector>,
    /// Per channel (mic, system), what it has been speaking.
    priors: [Vec<f32>; 2],
    filters: [ChannelFilter; 2],
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
                languages: languages.to_vec(),
                detector: None,
                priors: Default::default(),
                filters: Default::default(),
            });
        }
        let detector_model = detector_model
            .ok_or_else(|| SttError::Inference("a language detection model is required".into()))?;
        let mut detector = LanguageDetector::new(detector_model, languages)?;
        // A faster detector than the transcription model gets checked by it.
        if !Arc::ptr_eq(model, detector_model) {
            detector = detector.with_confirmation(model)?;
        }
        let uniform = vec![1.0 / languages.len() as f32; languages.len()];
        Ok(MeetingTranscriber {
            whisper,
            language: None,
            languages: languages.to_vec(),
            detector: Some(detector),
            priors: [uniform.clone(), uniform],
            filters: Default::default(),
        })
    }
}

impl Transcriber for MeetingTranscriber {
    fn transcribe(&mut self, chunk: &SpeechChunk) -> Result<Vec<Segment>, SttError> {
        let ch = match chunk.channel {
            Channel::Mic => 0,
            Channel::System => 1,
        };
        if self.filters[ch].too_quiet(chunk) {
            return Ok(Vec::new());
        }
        let mut segments = Vec::new();
        match &mut self.detector {
            None => segments = self.whisper.transcribe_in(chunk, self.language.as_deref())?,
            Some(detector) => {
                let prior = &mut self.priors[ch];
                for (part, lang) in detector.split_with(chunk, prior)? {
                    let new = self
                        .whisper
                        .transcribe_in(&part, Some(&detector.languages[lang].0))?;
                    // Only speech Whisper made sense of moves the prior; noise doesn't.
                    if !new.is_empty() {
                        update_prior(prior, lang, part.samples.len() as f32 / RATE as f32);
                    }
                    segments.extend(new);
                }
            }
        }
        if too_sparse(chunk, &segments) {
            return Ok(Vec::new());
        }
        let filter = &mut self.filters[ch];
        let languages = &self.languages;
        segments.retain(|s| !foreign_letters(&s.text, languages) && !filter.repeated(s));
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
        runs = merge_runs(runs);
    }
    runs
}

/// Joins neighboring runs in the same language.
fn merge_runs(runs: Vec<Run>) -> Vec<Run> {
    let mut merged: Vec<Run> = Vec::with_capacity(runs.len());
    for run in runs {
        match merged.last_mut() {
            Some(last) if last.label == run.label => last.end = run.end,
            _ => merged.push(run),
        }
    }
    merged
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
    fn short_audio_follows_the_channel_prior() {
        let spanish = [0.1, 0.9];
        // "ok" detected as English, but too short to outweigh a Spanish conversation.
        assert_eq!(decide(&[0.9, 0.1], &spanish, 1.0), 1);
        // A few seconds of clear English does switch.
        assert_eq!(decide(&[0.98, 0.02], &spanish, 4.0), 0);
        // Without a prior, the detector decides.
        assert_eq!(decide(&[0.7, 0.3], &[0.5, 0.5], 1.0), 0);
    }

    #[test]
    fn prior_moves_with_speech_but_never_closes() {
        let mut prior = vec![0.5, 0.5];
        for _ in 0..20 {
            update_prior(&mut prior, 1, 8.0);
        }
        assert!((prior[0] - MIN_PRIOR).abs() < 0.02, "{prior:?}");
        assert!((prior.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        update_prior(&mut prior, 0, 15.0);
        assert!(
            prior[0] > 0.5,
            "a long stretch of English moves it back: {prior:?}"
        );
    }

    #[test]
    fn cuts_at_the_quietest_frame() {
        let mut samples = vec![0.5; RATE];
        samples[8_000..8_400].fill(0.0);
        let cut = quietest_point(&samples, 0, RATE);
        assert!((8_000..=8_400 - CUT_FRAME).contains(&cut), "cut at {cut}");
    }
}
