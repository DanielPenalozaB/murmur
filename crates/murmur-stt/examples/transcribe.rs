//! Runs a 16 kHz mono WAV through the VAD segmenter and Whisper, like a live session.
//!
//!   cargo run --release -p murmur-stt --example transcribe -- recording.wav [en,es]

use std::path::PathBuf;
use std::time::Instant;

use murmur_audio::Segmenter;
use murmur_core::Channel;
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let wav = PathBuf::from(args.next().ok_or("usage: transcribe <file.wav> [en,es]")?);
    let languages: Vec<String> = args
        .next()
        .map_or(vec![], |l| l.split(',').map(String::from).collect());
    let models =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/dev.murmur.app/models");

    let t = Instant::now();
    let model = WhisperModel::load(&models.join("ggml-large-v3-turbo-q5_0.bin"))?;
    let detector = WhisperModel::load(&models.join("ggml-base.bin"))?;
    println!("models loaded in {:?}", t.elapsed());
    let mut transcriber = MeetingTranscriber::new(&model, Some(&detector), &languages)?;

    let samples: Vec<f32> = hound::WavReader::open(&wav)?
        .into_samples::<f32>()
        .collect::<Result<_, _>>()?;
    let mut segmenter = Segmenter::new(Channel::System);
    let mut chunks = Vec::new();
    for frame in samples.chunks(1_600) {
        chunks.extend(segmenter.push(frame));
    }
    chunks.extend(segmenter.flush());

    for chunk in &chunks {
        let t = Instant::now();
        let audio_ms = chunk.samples.len() * 1000 / 16_000;
        for s in transcriber.transcribe(chunk)? {
            println!("[{:>6} ms] {}", s.start, s.text);
        }
        println!("   ({audio_ms} ms audio in {:?})", t.elapsed());
    }
    Ok(())
}
