use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::thread::{self, JoinHandle};

use murmur_core::Channel;
use serde::Serialize;

use crate::{AudioError, HelperProcess, SAMPLE_RATE};

/// Records the mic and system audio into `<dir>/mic.wav` and `<dir>/system.wav`.
/// Paused audio is discarded, so file time equals session time.
pub struct Recorder {
    dir: PathBuf,
    paused: Arc<AtomicBool>,
    tracks: Vec<Track>,
}

struct Track {
    channel: Channel,
    path: PathBuf,
    helper: HelperProcess,
    writer: JoinHandle<Result<(), hound::Error>>,
    level: Arc<AtomicU32>,
    samples: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecorderStatus {
    pub paused: bool,
    pub elapsed_ms: u64,
    /// 0.0–1.0 meter values (−60 dBFS..0 dBFS).
    pub mic_level: f32,
    pub system_level: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recording {
    pub dir: PathBuf,
    pub tracks: Vec<TrackFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackFile {
    pub channel: Channel,
    pub path: PathBuf,
    pub duration_ms: u64,
}

impl Recorder {
    pub fn start(helper: &Path, dir: &Path) -> Result<Self, AudioError> {
        std::fs::create_dir_all(dir)?;
        let paused = Arc::new(AtomicBool::new(false));
        let mut tracks = Vec::new();
        // If the second channel fails, dropping `tracks` stops the first helper.
        for (channel, name) in [(Channel::Mic, "mic.wav"), (Channel::System, "system.wav")] {
            let (helper, frames) = HelperProcess::spawn(helper, channel)?;
            let path = dir.join(name);
            let wav = hound::WavWriter::create(&path, wav_spec())?;
            let level = Arc::new(AtomicU32::new(0));
            let samples = Arc::new(AtomicU64::new(0));
            let writer = spawn_writer(frames, wav, paused.clone(), level.clone(), samples.clone());
            tracks.push(Track {
                channel,
                path,
                helper,
                writer,
                level,
                samples,
            });
        }
        Ok(Recorder {
            dir: dir.to_path_buf(),
            paused,
            tracks,
        })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn status(&self) -> RecorderStatus {
        let level = |channel| {
            self.tracks
                .iter()
                .find(|t| t.channel == channel)
                .map_or(0.0, |t| f32::from_bits(t.level.load(Ordering::Relaxed)))
        };
        let elapsed_ms = self
            .tracks
            .iter()
            .map(|t| samples_to_ms(t.samples.load(Ordering::Relaxed)))
            .max()
            .unwrap_or(0);
        RecorderStatus {
            paused: self.paused.load(Ordering::Relaxed),
            elapsed_ms,
            mic_level: level(Channel::Mic),
            system_level: level(Channel::System),
        }
    }

    /// Stops both helpers and finalizes the WAV files.
    pub fn stop(mut self) -> Result<Recording, AudioError> {
        // Freeze both tracks at the same instant, then shut the helpers down in parallel.
        self.set_paused(true);
        for track in &mut self.tracks {
            track.helper.request_stop();
        }
        let mut files = Vec::new();
        for mut track in self.tracks {
            track.helper.stop();
            // The helper's stdout closed, so the frame channel closes and the writer ends.
            track.writer.join().expect("writer thread panicked")?;
            files.push(TrackFile {
                channel: track.channel,
                path: track.path,
                duration_ms: samples_to_ms(track.samples.load(Ordering::Relaxed)),
            });
        }
        Ok(Recording {
            dir: self.dir,
            tracks: files,
        })
    }
}

fn spawn_writer(
    frames: Receiver<Vec<f32>>,
    mut wav: hound::WavWriter<BufWriter<File>>,
    paused: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    samples: Arc<AtomicU64>,
) -> JoinHandle<Result<(), hound::Error>> {
    thread::spawn(move || {
        for frame in frames {
            level.store(meter(&frame).to_bits(), Ordering::Relaxed);
            if paused.load(Ordering::Relaxed) {
                continue;
            }
            for &sample in &frame {
                wav.write_sample(sample)?;
            }
            samples.fetch_add(frame.len() as u64, Ordering::Relaxed);
        }
        wav.finalize()
    })
}

fn wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    }
}

fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / SAMPLE_RATE as u64
}

/// RMS mapped from −60..0 dBFS to 0..1.
fn meter(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_maps_dbfs_to_unit_range() {
        assert_eq!(meter(&[]), 0.0);
        assert_eq!(meter(&[0.0; 160]), 0.0);
        assert!((meter(&[1.0; 160]) - 1.0).abs() < 1e-6);
        assert!((meter(&[0.001; 160]) - 0.0).abs() < 1e-3); // −60 dBFS
    }

    #[test]
    fn samples_convert_to_milliseconds() {
        assert_eq!(samples_to_ms(16_000), 1000);
        assert_eq!(samples_to_ms(1_600), 100);
    }
}
