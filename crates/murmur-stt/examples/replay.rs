//! Replays a recorded session through the same pipeline the app runs live (VAD →
//! language detection → Whisper → speaker identification), so changes can be tried on
//! a real meeting without recording it again.
//!
//!   cargo run --release -p murmur-stt --example replay -- <session-dir> [en,es] [from-s] [to-s]
//!
//! `session-dir` is a folder under `~/Library/Application Support/dev.murmur.app/sessions`.
//! The tracks there were volume-boosted after recording; the boost saved in
//! `audio.json` is undone so the pipeline hears what it heard live. Sessions from
//! before `audio.json` existed can pass `MURMUR_MIC_GAIN_DB` / `MURMUR_SYSTEM_GAIN_DB`.
//!
//! `MURMUR_STT_TRACE=1` prints every language decision and transcribed line with its
//! loudness and confidence.

use std::path::{Path, PathBuf};
use std::time::Instant;

use murmur_audio::{SAMPLE_RATE, read_wav, speech_chunks};
use murmur_core::{Channel, Segment};
use murmur_stt::speakers::{Diarizer, EMBEDDING_MODEL, label_segments};
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(
        args.first()
            .ok_or("usage: replay <session-dir> [en,es] [from-s] [to-s]")?,
    );
    let languages: Vec<String> = args
        .get(1)
        .map(|l| l.split(',').filter(|s| !s.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    let from = args.get(2).map_or(Ok(0.0), |s| s.parse::<f32>())?;
    let to = args.get(3).map_or(Ok(f32::INFINITY), |s| s.parse::<f32>())?;
    let models =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/dev.murmur.app/models");

    let turbo = WhisperModel::load(&models.join("ggml-large-v3-turbo-q5_0.bin"))?;
    let base = WhisperModel::load(&models.join("ggml-base.bin"))?;
    // MURMUR_DETECTOR=turbo identifies languages with the large model instead of base.
    let detector = if std::env::var("MURMUR_DETECTOR").as_deref() == Ok("turbo") {
        &turbo
    } else {
        &base
    };
    let mut transcriber = MeetingTranscriber::new(&turbo, Some(detector), &languages)?;

    let started = Instant::now();
    let mut segments: Vec<Segment> = Vec::new();
    let mut system = Vec::new();
    for (channel, file) in [(Channel::Mic, "mic.wav"), (Channel::System, "system.wav")] {
        let path = dir.join(file);
        if !path.exists() {
            continue;
        }
        let gain = live_gain(&dir, file.trim_end_matches(".wav"));
        let samples = clip(&read(&path)?, from, to);
        eprintln!(
            "{file}: {:.1} s, boost undone: {:.1} dB",
            samples.len() as f32 / SAMPLE_RATE as f32,
            20.0 * gain.log10()
        );
        let chunks = speech_chunks(channel, &samples, gain);
        for mut chunk in chunks {
            chunk.start_ms += (from * 1000.0) as u64;
            segments.extend(transcriber.transcribe(&chunk)?);
        }
        if channel == Channel::System {
            // Diarization doesn't depend on volume; it reads the boosted track in the app too.
            system = samples;
        }
    }
    let transcribed = started.elapsed();

    let t = Instant::now();
    let mut speakers = 0;
    if !system.is_empty() {
        // MURMUR_CLUSTER_THRESHOLD tries another speaker clustering threshold.
        let diarizer = match std::env::var("MURMUR_CLUSTER_THRESHOLD") {
            Ok(t) => Diarizer::load_tuned(&models.join("speakers"), EMBEDDING_MODEL, t.parse()?)?,
            Err(_) => Diarizer::load(&models.join("speakers"))?,
        };
        let mut diarization = diarizer.run(&system)?;
        let offset = (from * 1000.0) as u64;
        for turn in &mut diarization.turns {
            turn.start_ms += offset;
            turn.end_ms += offset;
        }
        speakers = label_segments(&mut segments, &diarization).len();
    }

    segments.sort_by_key(|s| s.start);
    for s in &segments {
        let who = match (&s.channel, &s.speaker) {
            (Channel::Mic, _) => "Me".to_string(),
            (_, Some(id)) => id.default_name(),
            _ => "Them".to_string(),
        };
        let at = s.start / 1000;
        println!("{:02}:{:02}  {who:<11} {}", at / 60, at % 60, s.text);
    }
    eprintln!(
        "{} segments, {speakers} speakers; transcribed in {transcribed:.1?}, speakers in {:.1?}",
        segments.len(),
        t.elapsed()
    );
    Ok(())
}

/// The volume boost applied to `channel`'s track after recording.
fn live_gain(dir: &Path, channel: &str) -> f32 {
    let env = format!("MURMUR_{}_GAIN_DB", channel.to_uppercase());
    if let Some(db) = std::env::var(env).ok().and_then(|v| v.parse::<f32>().ok()) {
        return 10f32.powf(db / 20.0);
    }
    std::fs::read(dir.join("audio.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["gains"][channel].as_f64())
        .map_or(1.0, |g| g as f32)
}

fn read(path: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    Ok(read_wav(path)?)
}

fn clip(samples: &[f32], from: f32, to: f32) -> Vec<f32> {
    let rate = SAMPLE_RATE as f32;
    let start = ((from * rate) as usize).min(samples.len());
    let end = if to.is_finite() {
        ((to * rate) as usize).min(samples.len())
    } else {
        samples.len()
    };
    samples[start..end.max(start)].to_vec()
}
