//! Prints who the speaker model hears in a session's system audio: talk time per
//! speaker and how alike their voiceprints are, for tuning how speakers are merged.
//!
//!   cargo run --release -p murmur-stt --example voices -- <session-dir>

use std::path::PathBuf;

use murmur_audio::read_wav;
use murmur_stt::speakers::{Diarizer, similarity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).ok_or("usage: voices <session-dir>")?);
    let models =
        PathBuf::from(std::env::var("HOME")?).join("Library/Application Support/dev.murmur.app/models");
    let samples = read_wav(&dir.join("system.wav"))?;
    let diarization = Diarizer::load(&models.join("speakers"))?.run(&samples)?;
    let count = diarization.voiceprints.len();
    let mut talk = vec![0u64; count];
    for t in &diarization.turns {
        talk[t.speaker] += t.end_ms - t.start_ms;
    }
    println!("{count} speakers");
    for (i, talk) in talk.iter().enumerate() {
        let alike: Vec<String> = (0..count)
            .filter(|&j| j != i)
            .map(|j| {
                (
                    similarity(&diarization.voiceprints[i], &diarization.voiceprints[j]),
                    j,
                )
            })
            .filter(|(s, _)| *s >= 0.35)
            .map(|(s, j)| format!("{}:{s:.2}", j + 1))
            .collect();
        println!(
            "Speaker {:>2} {:>6.1} s  {}",
            i + 1,
            *talk as f32 / 1000.0,
            alike.join(" ")
        );
    }
    Ok(())
}
