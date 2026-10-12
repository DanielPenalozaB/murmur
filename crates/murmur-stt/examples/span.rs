//! Transcribes one span of a WAV as a single chunk, to study chunk-level behavior.
//!
//!   MURMUR_PROMPT="..." cargo run --release -p murmur-stt --example span -- file.wav 20000 35000 [lang]

use std::path::PathBuf;

use murmur_audio::SpeechChunk;
use murmur_core::Channel;
use murmur_stt::{Transcriber, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [wav, start, end, rest @ ..] = args.as_slice() else {
        return Err("usage: span <file.wav> <start_ms> <end_ms> [lang]".into());
    };
    let (start, end): (usize, usize) = (start.parse()?, end.parse()?);
    let home = PathBuf::from(std::env::var("HOME")?);
    let model = WhisperModel::load(
        &home.join("Library/Application Support/dev.murmur.app/models/ggml-large-v3-turbo-q5_0.bin"),
    )?;
    let mut t = model.transcriber(rest.first().cloned())?;
    t.set_prompt(std::env::var("MURMUR_PROMPT").ok());

    let samples: Vec<f32> = hound::WavReader::open(wav)?.into_samples::<f32>().collect::<Result<_, _>>()?;
    let chunk = SpeechChunk {
        channel: Channel::Mic,
        start_ms: start as u64,
        samples: samples[start * 16..end.min(samples.len() / 16) * 16].to_vec(),
    };
    for s in t.transcribe(&chunk)? {
        println!("[{:>6} ms] {}", s.start, s.text);
    }
    Ok(())
}
