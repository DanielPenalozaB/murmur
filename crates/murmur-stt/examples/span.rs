//! Transcribes one span of a WAV as a single chunk, to study chunk-level behavior.
//!
//!   cargo run --release -p murmur-stt --example span -- file.wav 20000 35000 [en,es]
//!
//! Languages are a comma-separated list: one is forced, two or more split the chunk
//! where the language changes (as the app does). Omit them to let Whisper auto-detect.

use std::path::PathBuf;

use murmur_audio::SpeechChunk;
use murmur_core::Channel;
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [wav, start, end, rest @ ..] = args.as_slice() else {
        return Err("usage: span <file.wav> <start_ms> <end_ms> [en,es]".into());
    };
    let (start, end): (usize, usize) = (start.parse()?, end.parse()?);
    let models =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/dev.murmur.app/models");
    // MURMUR_MODEL=ggml-base.bin times the draft model instead.
    let file = std::env::var("MURMUR_MODEL").unwrap_or_else(|_| "ggml-large-v3-turbo-q5_0.bin".into());
    let model = WhisperModel::load(&models.join(file))?;
    let detector = WhisperModel::load(&models.join("ggml-base.bin"))?;
    let languages: Vec<String> = rest
        .first()
        .map_or(vec![], |l| l.split(',').map(String::from).collect());
    let mut t = MeetingTranscriber::new(&model, Some(&detector), &languages)?;

    let samples: Vec<f32> = hound::WavReader::open(wav)?
        .into_samples::<f32>()
        .collect::<Result<_, _>>()?;
    let chunk = SpeechChunk {
        channel: Channel::Mic,
        start_ms: start as u64,
        samples: samples[start * 16..end.min(samples.len() / 16) * 16].to_vec(),
    };
    let started = std::time::Instant::now();
    let segments = t.transcribe(&chunk)?;
    eprintln!("({:?})", started.elapsed());
    for s in segments {
        println!("[{:>6} ms] {}", s.start, s.text);
    }
    Ok(())
}
