//! Records mic + system audio for a few seconds without the UI.
//!
//!   ./scripts/build-audiocap.sh
//!   cargo run -p murmur-audio --example record -- /tmp/murmur-test 5

use std::path::PathBuf;
use std::time::Duration;

use murmur_audio::Recorder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "/tmp/murmur-test".into()));
    let seconds: u64 = args.next().map_or(Ok(5), |s| s.parse())?;
    let helper =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../native/audiocap/.build/release/audiocap");

    let recorder = Recorder::start(&helper, &dir, None)?;
    for _ in 0..seconds * 2 {
        std::thread::sleep(Duration::from_millis(500));
        let s = recorder.status();
        println!(
            "{:>6} ms  me {:.2}  them {:.2}",
            s.elapsed_ms, s.mic_level, s.system_level
        );
    }
    let recording = recorder.stop()?;
    for t in recording.tracks {
        println!("{:?}: {} ({} ms)", t.channel, t.path.display(), t.duration_ms);
    }
    Ok(())
}
