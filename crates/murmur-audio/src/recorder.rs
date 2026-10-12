use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use murmur_core::Channel;
use serde::Serialize;

use crate::{AudioError, HelperProcess, SAMPLE_RATE, Segmenter, SpeechChunk};

/// Records the mic and system audio into `<dir>/mic.wav` and `<dir>/system.wav`.
/// Paused audio is discarded, so file time equals session time. When given a chunk
/// sender, each track also runs a [`Segmenter`] and sends speech chunks to it.
pub struct Recorder {
    dir: PathBuf,
    started: Instant,
    paused: Arc<AtomicBool>,
    tracks: Vec<Track>,
}

struct Track {
    channel: Channel,
    path: PathBuf,
    helper: HelperProcess,
    writer: JoinHandle<Result<(), hound::Error>>,
    meters: Arc<Meters>,
}

/// Live values the writer thread publishes for the UI.
#[derive(Default)]
struct Meters {
    /// Frames received from the helper, paused or not. Stays 0 if capture is blocked.
    frames: AtomicU64,
    level: AtomicU32,
    samples: AtomicU64,
    speaking: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecorderStatus {
    pub paused: bool,
    /// Recorded time, excluding pauses.
    pub elapsed_ms: u64,
    /// Wall-clock time since start, including pauses.
    pub uptime_ms: u64,
    /// 0.0–1.0 meter values (−60 dBFS..0 dBFS).
    pub mic_level: f32,
    pub system_level: f32,
    /// Whether the VAD currently hears speech on that channel.
    pub mic_speaking: bool,
    pub system_speaking: bool,
    /// False until the channel's helper delivers its first frame. A channel that stays
    /// silent usually means a macOS permission prompt is pending or was denied.
    pub mic_receiving: bool,
    pub system_receiving: bool,
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
    pub fn start(helper: &Path, dir: &Path, chunks: Option<Sender<SpeechChunk>>) -> Result<Self, AudioError> {
        std::fs::create_dir_all(dir)?;
        let paused = Arc::new(AtomicBool::new(false));
        let mut tracks = Vec::new();
        // If the second channel fails, dropping `tracks` stops the first helper.
        for (channel, name) in [(Channel::Mic, "mic.wav"), (Channel::System, "system.wav")] {
            let (helper, frames) = HelperProcess::spawn(helper, channel)?;
            let path = dir.join(name);
            let meters = Arc::new(Meters::default());
            let writer = spawn_writer(TrackWriter {
                frames,
                wav: hound::WavWriter::create(&path, wav_spec())?,
                paused: paused.clone(),
                meters: meters.clone(),
                vad: chunks.clone().map(|tx| (Segmenter::new(channel), tx)),
            });
            tracks.push(Track {
                channel,
                path,
                helper,
                writer,
                meters,
            });
        }
        Ok(Recorder {
            dir: dir.to_path_buf(),
            started: Instant::now(),
            paused,
            tracks,
        })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn status(&self) -> RecorderStatus {
        let meters = |channel| {
            self.tracks
                .iter()
                .find(|t| t.channel == channel)
                .map(|t| &t.meters)
        };
        let level =
            |channel| meters(channel).map_or(0.0, |m| f32::from_bits(m.level.load(Ordering::Relaxed)));
        let speaking = |channel| meters(channel).is_some_and(|m| m.speaking.load(Ordering::Relaxed));
        let receiving = |channel| meters(channel).is_some_and(|m| m.frames.load(Ordering::Relaxed) > 0);
        let elapsed_ms = self
            .tracks
            .iter()
            .map(|t| samples_to_ms(t.meters.samples.load(Ordering::Relaxed)))
            .max()
            .unwrap_or(0);
        RecorderStatus {
            paused: self.paused.load(Ordering::Relaxed),
            elapsed_ms,
            uptime_ms: self.started.elapsed().as_millis() as u64,
            mic_level: level(Channel::Mic),
            system_level: level(Channel::System),
            mic_speaking: speaking(Channel::Mic),
            system_speaking: speaking(Channel::System),
            mic_receiving: receiving(Channel::Mic),
            system_receiving: receiving(Channel::System),
        }
    }

    /// Stops both helpers, flushes open speech chunks and finalizes the WAV files.
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
                duration_ms: samples_to_ms(track.meters.samples.load(Ordering::Relaxed)),
            });
        }
        Ok(Recording {
            dir: self.dir,
            tracks: files,
        })
    }
}

struct TrackWriter {
    frames: Receiver<Vec<f32>>,
    wav: hound::WavWriter<BufWriter<File>>,
    paused: Arc<AtomicBool>,
    meters: Arc<Meters>,
    vad: Option<(Segmenter, Sender<SpeechChunk>)>,
}

fn spawn_writer(mut t: TrackWriter) -> JoinHandle<Result<(), hound::Error>> {
    thread::spawn(move || {
        for frame in &t.frames {
            t.meters.frames.fetch_add(1, Ordering::Relaxed);
            t.meters.level.store(meter(&frame).to_bits(), Ordering::Relaxed);
            if t.paused.load(Ordering::Relaxed) {
                continue;
            }
            for &sample in &frame {
                t.wav.write_sample(sample)?;
            }
            t.meters.samples.fetch_add(frame.len() as u64, Ordering::Relaxed);
            if let Some((segmenter, tx)) = &mut t.vad {
                for chunk in segmenter.push(&frame) {
                    let _ = tx.send(chunk);
                }
                t.meters.speaking.store(segmenter.in_speech(), Ordering::Relaxed);
            }
        }
        if let Some((segmenter, tx)) = &mut t.vad
            && let Some(chunk) = segmenter.flush()
        {
            let _ = tx.send(chunk);
        }
        t.wav.finalize()
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
