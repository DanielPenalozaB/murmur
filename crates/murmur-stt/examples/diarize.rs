//! Finds who spoke when in a 16 kHz mono WAV (float or 16-bit), like the app does with
//! the system-audio track after a meeting.
//!
//!   cargo run --release -p murmur-stt --example diarize -- system.wav [embedding-model.onnx]

use std::path::PathBuf;
use std::time::Instant;

use murmur_stt::speakers::{Diarizer, EMBEDDING_MODEL, similarity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wav = args
        .first()
        .ok_or("usage: diarize <file.wav> [embedding-model.onnx]")?;
    let models = PathBuf::from(std::env::var("HOME")?)
        .join("Library/Application Support/dev.murmur.app/models/speakers");
    let embedding = args.get(1).map_or(EMBEDDING_MODEL, String::as_str);

    let mut reader = hound::WavReader::open(wav)?;
    let samples: Vec<f32> = match reader.spec().sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
            .collect::<Result<_, _>>()?,
    };

    let t = Instant::now();
    let diarizer = Diarizer::load_with(&models, embedding)?;
    println!("{embedding}: loaded in {:?}", t.elapsed());
    let t = Instant::now();
    let result = diarizer.run(&samples)?;
    println!(
        "{} speakers in {:?} for {:.1} s of audio",
        result.voiceprints.len(),
        t.elapsed(),
        samples.len() as f32 / 16_000.0
    );
    for turn in &result.turns {
        println!(
            "  {:>6.2}s – {:>6.2}s  Speaker {}",
            turn.start_ms as f32 / 1000.0,
            turn.end_ms as f32 / 1000.0,
            turn.speaker + 1
        );
    }
    for (i, a) in result.voiceprints.iter().enumerate() {
        let row: Vec<String> = result
            .voiceprints
            .iter()
            .map(|b| format!("{:.2}", similarity(a, b)))
            .collect();
        println!("  similarity Speaker {}: {}", i + 1, row.join("  "));
    }
    Ok(())
}
