//! Transcribes fixed windows of a WAV with one language, to see what Whisper hears
//! without the VAD in the way.
//!
//!   cargo run --release -p murmur-stt --example probe -- file.wav from-s to-s [lang] [window-s]

use std::path::PathBuf;

use murmur_audio::{SAMPLE_RATE, SpeechChunk, read_wav};
use murmur_core::Channel;
use murmur_stt::{LanguageDetector, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let samples = read_wav(&PathBuf::from(&args[0]))?;
    let from: f32 = args[1].parse()?;
    let to: f32 = args[2].parse()?;
    let lang = args.get(3).cloned().filter(|l| l != "auto");
    let window: f32 = args.get(4).map_or(Ok(30.0), |s| s.parse())?;
    let models =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/dev.murmur.app/models");
    let turbo = WhisperModel::load(&models.join("ggml-large-v3-turbo-q5_0.bin"))?;
    let mut whisper = turbo.transcriber(None)?;
    let mut detector = LanguageDetector::new(&turbo, &["en".into(), "es".into()])?;
    let rate = SAMPLE_RATE as f32;
    let mut at = from;
    while at < to {
        let end = (at + window).min(to);
        let part = &samples[(at * rate) as usize..((end * rate) as usize).min(samples.len())];
        let rms = (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt();
        let probs = detector.probabilities(&part[..part.len().min(30 * SAMPLE_RATE as usize)])?;
        let chunk = SpeechChunk {
            channel: Channel::System,
            start_ms: (at * 1000.0) as u64,
            samples: part.to_vec(),
        };
        let text: Vec<String> = whisper
            .transcribe_in(&chunk, lang.as_deref())?
            .into_iter()
            .map(|s| s.text)
            .collect();
        println!(
            "{at:>5.0}-{end:<5.0} {:>4.0} dB  en={:.2} es={:.2}  {}",
            20.0 * rms.max(1e-9).log10(),
            probs[0],
            probs[1],
            text.join(" ")
        );
        at = end;
    }
    Ok(())
}
