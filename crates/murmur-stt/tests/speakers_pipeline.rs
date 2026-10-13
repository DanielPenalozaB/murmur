//! End-to-end: a generated three-person meeting goes through the same steps as a real
//! one (VAD chunks → Whisper → diarization → speaker labels), then a second meeting
//! with the same voices is named from the first one's voiceprints.
//!
//! Needs the downloaded models and macOS `say`, so it's ignored by default:
//!
//!   cargo test --release -p murmur-stt --test speakers_pipeline -- --ignored

use std::path::{Path, PathBuf};
use std::process::Command;

use murmur_audio::Segmenter;
use murmur_core::{Channel, Segment};
use murmur_stt::speakers::{self, Diarizer, VOICE_MATCH_THRESHOLD};
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};

const ANA: &str = "Daniel";
const BEA: &str = "Flo (Spanish (Mexico))";
const CARL: &str = "Fred";

fn models() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/dev.murmur.app/models")
}

/// Speaks each line with its voice and joins them with short pauses, like a call.
fn meeting(dir: &Path, lines: &[(&str, &str)]) -> Vec<f32> {
    let mut samples = Vec::new();
    for (i, (voice, text)) in lines.iter().enumerate() {
        let path = dir.join(format!("{i}.wav"));
        let status = Command::new("say")
            .args(["-v", voice, "-o"])
            .arg(&path)
            .args(["--file-format=WAVE", "--data-format=LEI16@16000", text])
            .status()
            .expect("macOS say");
        assert!(status.success());
        let mut reader = hound::WavReader::open(&path).unwrap();
        samples.extend(reader.samples::<i16>().map(|s| s.unwrap() as f32 / i16::MAX as f32));
        samples.extend(std::iter::repeat_n(0.0, 16_000 * 7 / 10));
    }
    samples
}

fn transcribe(samples: &[f32], transcriber: &mut MeetingTranscriber) -> Vec<Segment> {
    let mut segmenter = Segmenter::new(Channel::System);
    let mut chunks = Vec::new();
    for frame in samples.chunks(1_600) {
        chunks.extend(segmenter.push(frame));
    }
    chunks.extend(segmenter.flush());
    chunks.iter().flat_map(|c| transcriber.transcribe(c).unwrap()).collect()
}

/// The speaker id of the segment containing `words`.
fn speaker_of(segments: &[Segment], words: &str) -> String {
    let segment = segments
        .iter()
        .find(|s| s.text.to_lowercase().contains(words))
        .unwrap_or_else(|| panic!("no segment contains {words:?}: {segments:#?}"));
    segment.speaker.as_ref().expect("labelled").0.clone()
}

#[test]
#[ignore = "needs downloaded models and macOS say"]
fn identifies_and_remembers_speakers() {
    let dir = tempfile::tempdir().unwrap();
    let whisper = WhisperModel::load(&models().join("ggml-large-v3-turbo-q5_0.bin")).unwrap();
    let base = WhisperModel::load(&models().join("ggml-base.bin")).unwrap();
    let languages = ["en".to_string(), "es".to_string()];
    let mut transcriber = MeetingTranscriber::new(&whisper, Some(&base), &languages).unwrap();
    let diarizer = Diarizer::load(&models().join("speakers")).unwrap();

    let first = meeting(
        dir.path(),
        &[
            (ANA, "Good morning everyone, let's start with the deployment status for this sprint."),
            (BEA, "Buenos días. El despliegue está casi listo, solo falta la revisión de seguridad."),
            (CARL, "Okay, and do we know when the security review will be finished?"),
            (ANA, "I talked to the security team yesterday, they expect to finish on Thursday."),
            (BEA, "Perfecto, entonces podemos lanzar el viernes por la mañana."),
            (CARL, "Sounds good to me. I'll update the release notes for the product team."),
        ],
    );
    let mut segments = transcribe(&first, &mut transcriber);
    let voiceprints = speakers::label_segments(&mut segments, &diarizer.run(&first).unwrap());

    assert_eq!(voiceprints.len(), 3, "{segments:#?}");
    let ana = speaker_of(&segments, "deployment status");
    let bea = speaker_of(&segments, "revisión de seguridad");
    let carl = speaker_of(&segments, "security review");
    assert_eq!(ana, "speaker_1", "speaks first");
    assert_eq!(speaker_of(&segments, "security team"), ana);
    assert_eq!(speaker_of(&segments, "viernes"), bea);
    assert_eq!(speaker_of(&segments, "release notes"), carl);
    assert!(ana != bea && bea != carl && ana != carl);

    // The user names them; the next meeting (different words, different order) is
    // named from those voices.
    let index = |id: &str| id.trim_start_matches("speaker_").parse::<usize>().unwrap() - 1;
    let known = vec![
        ("Ana".to_string(), voiceprints[index(&ana)].clone()),
        ("Bea".to_string(), voiceprints[index(&bea)].clone()),
        ("Carl".to_string(), voiceprints[index(&carl)].clone()),
    ];
    let second = meeting(
        dir.path(),
        &[
            (CARL, "Quick update from my side, the release notes are done and shared."),
            (ANA, "Excellent. Then the only open item is the security sign off."),
            (BEA, "Sí, pongamos quince minutos a las nueve para confirmar que todo está en orden."),
        ],
    );
    let mut segments = transcribe(&second, &mut transcriber);
    let voiceprints = speakers::label_segments(&mut segments, &diarizer.run(&second).unwrap());
    let names = speakers::match_voices(&voiceprints, &known, VOICE_MATCH_THRESHOLD);
    let name_of = |words: &str| names[index(&speaker_of(&segments, words))].clone();
    assert_eq!(name_of("release notes").as_deref(), Some("Carl"));
    assert_eq!(name_of("sign off").as_deref(), Some("Ana"));
    assert_eq!(name_of("confirmar").as_deref(), Some("Bea"));
}
