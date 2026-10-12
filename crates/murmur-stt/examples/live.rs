//! Live pipeline without the UI: record mic + system audio and print segments as
//! they're transcribed.
//!
//!   ./scripts/build-audiocap.sh
//!   cargo run --release -p murmur-stt --example live -- 20

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use murmur_audio::Recorder;
use murmur_stt::{Transcriber, WhisperModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args().nth(1).map_or(Ok(20), |s| s.parse())?;
    let home = PathBuf::from(std::env::var("HOME")?);
    let model = WhisperModel::load(
        &home.join("Library/Application Support/dev.murmur.app/models/ggml-large-v3-turbo-q5_0.bin"),
    )?;
    let mut transcriber = model.transcriber(None)?;
    let helper =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../native/audiocap/.build/release/audiocap");
    let dir = std::env::temp_dir().join("murmur-live");

    let (tx, rx) = mpsc::channel();
    let recorder = Recorder::start(&helper, &dir, Some(tx))?;
    let began = Instant::now();
    let worker = std::thread::spawn(move || {
        for chunk in rx {
            for s in transcriber.transcribe(&chunk).expect("transcribe") {
                println!(
                    "[{:>6} ms] {:?}: {}   (shown at {:.1}s)",
                    s.start,
                    s.channel,
                    s.text,
                    began.elapsed().as_secs_f32()
                );
            }
        }
    });
    for _ in 0..seconds * 2 {
        std::thread::sleep(Duration::from_millis(500));
        let st = recorder.status();
        eprintln!(
            "status: {} ms  paused={}  me {:.2}  them {:.2}",
            st.elapsed_ms, st.paused, st.mic_level, st.system_level
        );
    }
    recorder.stop()?;
    worker.join().unwrap();
    Ok(())
}
