//! Who spoke when on the system-audio channel (everyone but the user), after the
//! meeting. sherpa-onnx runs pyannote segmentation, then clusters speaker embeddings.
//! Each speaker also gets one embedding (a voiceprint) so names can be remembered
//! across meetings.

use std::path::Path;

use murmur_audio::SAMPLE_RATE;
use murmur_core::{Channel, Segment, SpeakerId};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
};

use crate::SttError;

/// File names inside the `speakers` models folder.
pub const SEGMENTATION_MODEL: &str = "sherpa-onnx-pyannote-segmentation-3-0/model.onnx";
/// TitaNet separated all three voices in a test where WeSpeaker merged two.
pub const EMBEDDING_MODEL: &str = "nemo_en_titanet_small.onnx";

/// Similarity above which a speaker counts as a remembered voice. The same synthetic
/// voice scored 0.96 across two recordings and different voices at most 0.37; real
/// voices through different mics vary more.
pub const VOICE_MATCH_THRESHOLD: f32 = 0.6;

/// Cosine distance under which two stretches of speech count as the same person.
/// Higher merges more, but at 0.8 two of the test voices merged. Clustering alone
/// split a 14-minute call with about eight people into 60 speakers; most were
/// fragments, which [`absorb_fragments`] folds back in.
const CLUSTER_THRESHOLD: f32 = 0.5;
/// Speakers with less talk than this are usually fragments of someone else (a laugh,
/// crosstalk, a bad mic moment) ...
const FRAGMENT_MS: u64 = 15_000;
/// ... and join the main speaker they sound most like, when at least this similar.
/// Main speakers in that call were at most 0.56 alike, fragments 0.56–0.82 to theirs.
const FRAGMENT_MATCH: f32 = 0.5;
/// Scraps shorter than this join the closest main speaker however unlike: a wrong
/// guess for a "sí" beats a "Speaker 23" who said one word.
const SCRAP_MS: u64 = 5_000;
/// Audio used per speaker for their voiceprint, longest turns first.
const VOICEPRINT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurn {
    pub start_ms: u64,
    pub end_ms: u64,
    /// 0-based, in order of first appearance.
    pub speaker: usize,
}

#[derive(Debug, Clone)]
pub struct Diarization {
    pub turns: Vec<SpeakerTurn>,
    /// One L2-normalized voiceprint per speaker, indexed like `SpeakerTurn::speaker`.
    pub voiceprints: Vec<Vec<f32>>,
}

pub struct Diarizer {
    diarization: OfflineSpeakerDiarization,
    extractor: SpeakerEmbeddingExtractor,
}

impl Diarizer {
    /// `models` is the `speakers` models folder.
    pub fn load(models: &Path) -> Result<Self, SttError> {
        Self::load_with(models, EMBEDDING_MODEL)
    }

    /// Like [`Diarizer::load`] with another embedding model file, for comparing models.
    pub fn load_with(models: &Path, embedding_model: &str) -> Result<Self, SttError> {
        Self::load_tuned(models, embedding_model, CLUSTER_THRESHOLD)
    }

    /// Like [`Diarizer::load_with`] with another clustering threshold, for tuning.
    pub fn load_tuned(models: &Path, embedding_model: &str, threshold: f32) -> Result<Self, SttError> {
        let segmentation = models.join(SEGMENTATION_MODEL);
        let embedding = models.join(embedding_model);
        for path in [&segmentation, &embedding] {
            if !path.exists() {
                return Err(SttError::ModelMissing(path.display().to_string()));
            }
        }
        let embedding = SpeakerEmbeddingExtractorConfig {
            model: Some(embedding.to_string_lossy().into_owned()),
            num_threads: 4,
            ..Default::default()
        };
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(segmentation.to_string_lossy().into_owned()),
                    ..Default::default()
                },
                num_threads: 4,
                ..Default::default()
            },
            embedding: embedding.clone(),
            clustering: FastClusteringConfig {
                threshold,
                ..Default::default()
            },
            ..Default::default()
        };
        let diarization = OfflineSpeakerDiarization::create(&config)
            .ok_or_else(|| SttError::Inference("could not load the speaker models".into()))?;
        let extractor = SpeakerEmbeddingExtractor::create(&embedding)
            .ok_or_else(|| SttError::Inference("could not load the speaker embedding model".into()))?;
        Ok(Diarizer {
            diarization,
            extractor,
        })
    }

    /// `samples` is the whole system-audio track at 16 kHz.
    pub fn run(&self, samples: &[f32]) -> Result<Diarization, SttError> {
        let result = self
            .diarization
            .process(samples)
            .ok_or_else(|| SttError::Inference("speaker detection failed".into()))?;
        let raw: Vec<(u64, u64, i32)> = result
            .sort_by_start_time()
            .into_iter()
            .map(|s| ((s.start * 1000.0) as u64, (s.end * 1000.0) as u64, s.speaker))
            .collect();

        let mut turns = renumber(&raw);
        let count = turns.iter().map(|t| t.speaker + 1).max().unwrap_or(0);
        let voiceprints: Vec<Vec<f32>> = (0..count)
            .map(|speaker| self.voiceprint(samples, &turns, speaker))
            .collect::<Result<_, _>>()?;

        let into = absorb_fragments(&turns, &voiceprints);
        if into.iter().enumerate().all(|(i, &j)| i == j) {
            return Ok(Diarization { turns, voiceprints });
        }
        let merged: Vec<(u64, u64, i32)> = turns
            .iter()
            .map(|t| (t.start_ms, t.end_ms, into[t.speaker] as i32))
            .collect();
        turns = renumber(&merged);
        let count = turns.iter().map(|t| t.speaker + 1).max().unwrap_or(0);
        let voiceprints = (0..count)
            .map(|speaker| self.voiceprint(samples, &turns, speaker))
            .collect::<Result<_, _>>()?;
        Ok(Diarization { turns, voiceprints })
    }

    fn voiceprint(
        &self,
        samples: &[f32],
        turns: &[SpeakerTurn],
        speaker: usize,
    ) -> Result<Vec<f32>, SttError> {
        let mut own: Vec<&SpeakerTurn> = turns.iter().filter(|t| t.speaker == speaker).collect();
        own.sort_by_key(|t| std::cmp::Reverse(t.end_ms - t.start_ms));
        let stream = self
            .extractor
            .create_stream()
            .ok_or_else(|| SttError::Inference("could not create an embedding stream".into()))?;
        let mut used = 0;
        for t in own {
            if used >= VOICEPRINT_MS {
                break;
            }
            let (start, end) = (
                ms_to_sample(t.start_ms),
                ms_to_sample(t.end_ms).min(samples.len()),
            );
            if start < end {
                stream.accept_waveform(SAMPLE_RATE as i32, &samples[start..end]);
                used += t.end_ms - t.start_ms;
            }
        }
        stream.input_finished();
        if !self.extractor.is_ready(&stream) {
            return Ok(Vec::new());
        }
        let embedding = self
            .extractor
            .compute(&stream)
            .ok_or_else(|| SttError::Inference("could not compute a voiceprint".into()))?;
        Ok(normalize(embedding))
    }
}

/// Numbers speakers 0, 1, … by first appearance, so "Speaker 1" talks first.
fn renumber(raw: &[(u64, u64, i32)]) -> Vec<SpeakerTurn> {
    let mut order: Vec<i32> = Vec::new();
    raw.iter()
        .map(|&(start_ms, end_ms, label)| {
            let speaker = order.iter().position(|&l| l == label).unwrap_or_else(|| {
                order.push(label);
                order.len() - 1
            });
            SpeakerTurn {
                start_ms,
                end_ms,
                speaker,
            }
        })
        .collect()
}

/// For each speaker, who they really are: themselves, or the main speaker (one with
/// [`FRAGMENT_MS`] of talk or more) a fragment sounds most like.
fn absorb_fragments(turns: &[SpeakerTurn], voiceprints: &[Vec<f32>]) -> Vec<usize> {
    let mut talk = vec![0u64; voiceprints.len()];
    for t in turns {
        talk[t.speaker] += t.end_ms - t.start_ms;
    }
    let main: Vec<usize> = (0..talk.len()).filter(|&i| talk[i] >= FRAGMENT_MS).collect();
    (0..talk.len())
        .map(|i| {
            if talk[i] >= FRAGMENT_MS {
                return i;
            }
            main.iter()
                .map(|&j| (similarity(&voiceprints[i], &voiceprints[j]), j))
                .filter(|&(score, _)| score >= FRAGMENT_MATCH || talk[i] < SCRAP_MS)
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map_or(i, |(_, j)| j)
        })
        .collect()
}

fn ms_to_sample(ms: u64) -> usize {
    (ms * SAMPLE_RATE as u64 / 1000) as usize
}

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// Cosine similarity of two L2-normalized voiceprints (1.0 = same direction).
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// `speaker_1`, `speaker_2`… for 0-based speaker indices.
pub fn speaker_id(speaker: usize) -> SpeakerId {
    SpeakerId(format!("speaker_{}", speaker + 1))
}

/// Gives each system-audio segment the speaker who talks most during it. Segments
/// with no overlapping turn keep `None` ("Them").
pub fn assign_speakers(segments: &mut [Segment], turns: &[SpeakerTurn]) {
    for segment in segments.iter_mut().filter(|s| s.channel == Channel::System) {
        let mut overlap: Vec<(usize, u64)> = Vec::new();
        for t in turns {
            let o = segment
                .end
                .min(t.end_ms)
                .saturating_sub(segment.start.max(t.start_ms));
            if o == 0 {
                continue;
            }
            match overlap.iter_mut().find(|(s, _)| *s == t.speaker) {
                Some((_, total)) => *total += o,
                None => overlap.push((t.speaker, o)),
            }
        }
        segment.speaker = overlap
            .into_iter()
            .max_by_key(|&(_, total)| total)
            .map(|(speaker, _)| speaker_id(speaker));
    }
}

/// Assigns speakers to system-audio segments, then numbers them by first appearance
/// in the transcript, dropping speakers no segment was assigned to (noise, or speech
/// Whisper didn't transcribe). Returns the voiceprints of the remaining speakers:
/// index `i` belongs to `speaker_{i+1}`.
pub fn label_segments(segments: &mut [Segment], diarization: &Diarization) -> Vec<Vec<f32>> {
    assign_speakers(segments, &diarization.turns);
    let mut order: Vec<String> = Vec::new();
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by_key(|s| s.start);
    for s in sorted {
        if let Some(id) = &s.speaker
            && !order.contains(&id.0)
        {
            order.push(id.0.clone());
        }
    }
    for s in segments.iter_mut() {
        if let Some(id) = &s.speaker {
            let n = order.iter().position(|o| *o == id.0).expect("collected above");
            s.speaker = Some(speaker_id(n));
        }
    }
    order
        .iter()
        .map(|old| {
            let index: usize = old
                .trim_start_matches("speaker_")
                .parse::<usize>()
                .expect("our own ids")
                - 1;
            diarization.voiceprints.get(index).cloned().unwrap_or_default()
        })
        .collect()
}

/// Matches this meeting's speakers to remembered voices. Each known name is used at
/// most once, best match first. Returns a name (or `None`) per speaker.
pub fn match_voices(
    voiceprints: &[Vec<f32>],
    known: &[(String, Vec<f32>)],
    threshold: f32,
) -> Vec<Option<String>> {
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (s, v) in voiceprints.iter().enumerate() {
        for (k, (_, known_v)) in known.iter().enumerate() {
            let score = similarity(v, known_v);
            if score >= threshold {
                pairs.push((score, s, k));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut names = vec![None; voiceprints.len()];
    let mut taken = vec![false; known.len()];
    for (_, s, k) in pairs {
        if names[s].is_none() && !taken[k] {
            names[s] = Some(known[k].0.clone());
            taken[k] = true;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: u64, end: u64, channel: Channel) -> Segment {
        Segment {
            start,
            end,
            channel,
            speaker: None,
            text: String::new(),
        }
    }

    fn turn(start_ms: u64, end_ms: u64, speaker: usize) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    #[test]
    fn assigns_the_speaker_with_most_overlap() {
        let mut segments = [
            seg(0, 4_000, Channel::System),
            seg(4_500, 9_000, Channel::System),
            seg(0, 2_000, Channel::Mic),
            seg(20_000, 21_000, Channel::System),
        ];
        let turns = [turn(0, 3_000, 0), turn(3_000, 9_000, 1)];
        assign_speakers(&mut segments, &turns);
        assert_eq!(segments[0].speaker, Some(speaker_id(0)));
        assert_eq!(segments[1].speaker, Some(speaker_id(1)));
        assert_eq!(segments[2].speaker, None, "the mic is always the user");
        assert_eq!(segments[3].speaker, None, "no turn overlaps");
    }

    #[test]
    fn numbers_speakers_by_first_appearance_in_the_transcript() {
        // Diarization found three speakers, but speaker index 0 only made noise.
        let diarization = Diarization {
            turns: vec![turn(0, 1_000, 0), turn(1_000, 5_000, 2), turn(5_000, 9_000, 1)],
            voiceprints: vec![vec![0.0], vec![1.0], vec![2.0]],
        };
        let mut segments = [
            seg(5_500, 8_500, Channel::System),
            seg(1_500, 4_500, Channel::System),
        ];
        let voiceprints = label_segments(&mut segments, &diarization);
        assert_eq!(segments[1].speaker, Some(speaker_id(0)), "talks first");
        assert_eq!(segments[0].speaker, Some(speaker_id(1)));
        assert_eq!(voiceprints, [vec![2.0], vec![1.0]]);
    }

    #[test]
    fn fragments_join_the_main_speaker_they_sound_like() {
        let ana = normalize(vec![1.0, 0.0, 0.0]);
        let bo = normalize(vec![0.0, 1.0, 0.0]);
        let turns = [
            turn(0, 30_000, 0),
            turn(30_000, 60_000, 1),
            turn(60_000, 62_000, 2), // sounds like Bo
            turn(62_000, 63_000, 3), // a scrap that sounds like nobody
            turn(63_000, 71_000, 4), // a fragment that sounds like nobody
        ];
        let voiceprints = [
            ana,
            bo,
            normalize(vec![0.2, 0.9, 0.1]),
            normalize(vec![0.3, 0.0, 1.0]),
            normalize(vec![0.0, 0.0, 1.0]),
        ];
        assert_eq!(absorb_fragments(&turns, &voiceprints), [0, 1, 1, 0, 4]);
    }

    #[test]
    fn matches_each_known_voice_once() {
        let ana = normalize(vec![1.0, 0.1, 0.0]);
        let bo = normalize(vec![0.0, 1.0, 0.1]);
        let known = [("Ana".to_string(), ana.clone()), ("Bo".to_string(), bo.clone())];
        let speakers = [
            normalize(vec![0.9, 0.2, 0.0]),
            normalize(vec![1.0, 0.0, 0.05]),
            normalize(vec![0.0, 0.0, 1.0]),
        ];
        let names = match_voices(&speakers, &known, 0.8);
        // Speaker 2 is the closer match for Ana, so speaker 1 doesn't get the name too.
        assert_eq!(names, [None, Some("Ana".to_string()), None]);
    }
}
