use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use murmur_core::Channel;
use serde::Serialize;

use crate::{AudioError, HelperProcess, SAMPLE_RATE, Segmenter, SpeechChunk};

/// Records the mic and system audio into `<dir>/mic.wav` and `<dir>/system.wav`.
/// Paused audio is discarded, so file time equals session time. When given
/// [`ChunkSenders`], each track also runs a [`Segmenter`] and sends speech chunks.
///
/// Both tracks follow the wall clock: when a helper stalls (the mic switching
/// devices) or dies and is restarted, the gap is filled with silence so the mic and
/// system audio stay in step.
pub struct Recorder {
    dir: PathBuf,
    started: Instant,
    paused: Arc<AtomicBool>,
    tracks: Vec<Track>,
}

struct Track {
    channel: Channel,
    path: PathBuf,
    supervisor: Supervisor,
    writer: JoinHandle<Result<(), hound::Error>>,
    meters: Arc<Meters>,
    muted: Arc<AtomicBool>,
}

/// A gap this long in a helper's audio is filled with silence.
const MAX_GAP_MS: u64 = 500;
/// Wait between attempts to restart a helper that exited.
const RESTART_DELAY: Duration = Duration::from_secs(1);

/// Keeps one channel's helper running for the whole recording, restarting it if it
/// exits, and forwards its frames to the writer over one channel that outlives it.
struct Supervisor {
    helper: Arc<Mutex<HelperProcess>>,
    stopping: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Supervisor {
    fn start(
        path: &Path,
        channel: Channel,
        meters: Arc<Meters>,
    ) -> Result<(Self, Receiver<Vec<f32>>), AudioError> {
        let (helper, mut frames) = HelperProcess::spawn(path, channel)?;
        let helper = Arc::new(Mutex::new(helper));
        let stopping = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let thread = {
            let (helper, stopping, path) = (helper.clone(), stopping.clone(), path.to_path_buf());
            thread::spawn(move || {
                loop {
                    for frame in &frames {
                        if tx.send(frame).is_err() {
                            return;
                        }
                    }
                    // The helper exited. Expected on stop; otherwise bring it back.
                    loop {
                        if stopping.load(Ordering::Relaxed) {
                            return;
                        }
                        tracing::warn!(?channel, "capture stopped unexpectedly; restarting");
                        thread::sleep(RESTART_DELAY);
                        if stopping.load(Ordering::Relaxed) {
                            return;
                        }
                        match HelperProcess::spawn(&path, channel) {
                            Ok((restarted, new_frames)) => {
                                *helper.lock().unwrap() = restarted;
                                frames = new_frames;
                                meters.restarts.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                            Err(e) => tracing::warn!(?channel, "restart failed: {e}"),
                        }
                    }
                }
            })
        };
        Ok((
            Supervisor {
                helper,
                stopping,
                thread,
            },
            rx,
        ))
    }

    fn request_stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        self.helper.lock().unwrap().request_stop();
    }

    fn stop(self) {
        self.request_stop();
        self.helper.lock().unwrap().stop();
        let _ = self.thread.join();
    }
}

/// Live values the writer thread publishes for the UI.
#[derive(Default)]
struct Meters {
    /// Frames received from the helper, paused or not. Stays 0 if capture is blocked.
    frames: AtomicU64,
    level: AtomicU32,
    samples: AtomicU64,
    speaking: AtomicBool,
    /// Times the helper had to be restarted.
    restarts: AtomicU32,
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
    /// The user turned the mic off in Murmur; silence is recorded instead.
    pub mic_muted: bool,
    /// How often capture had to be restarted (device changes, crashes).
    pub restarts: u32,
}

/// Where speech goes for transcription.
#[derive(Clone)]
pub struct ChunkSenders {
    /// Finished utterances, cut at pauses.
    pub finals: Sender<SpeechChunk>,
    /// While someone is still talking, the utterance so far every [`DRAFT_EVERY_MS`].
    pub drafts: Option<Sender<SpeechChunk>>,
}

pub const DRAFT_EVERY_MS: u64 = 1_500;

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
    /// Volume boost applied after recording (see [`crate::normalize_wav`]), so the
    /// raw signal can be recovered, e.g. to replay the session exactly as heard live.
    pub gain: f32,
}

impl Recorder {
    pub fn start(helper: &Path, dir: &Path, chunks: Option<ChunkSenders>) -> Result<Self, AudioError> {
        std::fs::create_dir_all(dir)?;
        let paused = Arc::new(AtomicBool::new(false));
        // Both tracks measure time from here, so they line up even though the
        // helpers start one after the other.
        let started = Instant::now();
        let mut tracks: Vec<Track> = Vec::new();
        for (channel, name) in [(Channel::Mic, "mic.wav"), (Channel::System, "system.wav")] {
            let meters = Arc::new(Meters::default());
            let (supervisor, frames) = match Supervisor::start(helper, channel, meters.clone()) {
                Ok(started) => started,
                Err(e) => {
                    for track in tracks {
                        track.supervisor.stop();
                    }
                    return Err(e);
                }
            };
            let path = dir.join(name);
            let muted = Arc::new(AtomicBool::new(false));
            let writer = spawn_writer(TrackWriter {
                frames,
                wav: hound::WavWriter::create(&path, wav_spec())?,
                started,
                paused: paused.clone(),
                muted: muted.clone(),
                meters: meters.clone(),
                vad: chunks.clone().map(|senders| Vad {
                    segmenter: Segmenter::new(channel),
                    senders,
                    since_draft: 0,
                }),
                written: 0,
            });
            tracks.push(Track {
                channel,
                path,
                supervisor,
                writer,
                meters,
                muted,
            });
        }
        Ok(Recorder {
            dir: dir.to_path_buf(),
            started,
            paused,
            tracks,
        })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Stops listening to `channel` (the user muted their mic in Murmur) or resumes.
    /// Time keeps running: silence is recorded meanwhile, and nothing is transcribed.
    pub fn set_muted(&self, channel: Channel, muted: bool) {
        if let Some(track) = self.tracks.iter().find(|t| t.channel == channel) {
            track.muted.store(muted, Ordering::Relaxed);
        }
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
            mic_muted: self
                .tracks
                .iter()
                .any(|t| t.channel == Channel::Mic && t.muted.load(Ordering::Relaxed)),
            restarts: self
                .tracks
                .iter()
                .map(|t| t.meters.restarts.load(Ordering::Relaxed))
                .sum(),
        }
    }

    /// Stops both helpers, flushes open speech chunks, finalizes the WAV files and
    /// normalizes their volume.
    pub fn stop(self) -> Result<Recording, AudioError> {
        // Freeze both tracks at the same instant, then shut the helpers down in parallel.
        self.set_paused(true);
        for track in &self.tracks {
            track.supervisor.request_stop();
        }
        let mut files = Vec::new();
        for track in self.tracks {
            track.supervisor.stop();
            // The supervisor dropped its sender, so the writer's channel closes and it ends.
            track.writer.join().expect("writer thread panicked")?;
            let gain = crate::normalize_wav(&track.path)?;
            files.push(TrackFile {
                channel: track.channel,
                path: track.path,
                duration_ms: samples_to_ms(track.meters.samples.load(Ordering::Relaxed)),
                gain,
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
    started: Instant,
    paused: Arc<AtomicBool>,
    muted: Arc<AtomicBool>,
    meters: Arc<Meters>,
    vad: Option<Vad>,
    /// Samples written so far.
    written: u64,
}

struct Vad {
    segmenter: Segmenter,
    senders: ChunkSenders,
    /// Samples recorded since the last draft was sent.
    since_draft: u64,
}

impl TrackWriter {
    /// Appends audio to the file and the VAD.
    fn record(&mut self, frame: &[f32]) -> Result<(), hound::Error> {
        for &sample in frame {
            self.wav.write_sample(sample)?;
        }
        self.written += frame.len() as u64;
        self.meters.samples.store(self.written, Ordering::Relaxed);
        if let Some(vad) = &mut self.vad {
            for chunk in vad.segmenter.push(frame) {
                let _ = vad.senders.finals.send(chunk);
                vad.since_draft = 0;
            }
            self.meters
                .speaking
                .store(vad.segmenter.in_speech(), Ordering::Relaxed);
            vad.since_draft += frame.len() as u64;
            if let Some(drafts) = &vad.senders.drafts
                && samples_to_ms(vad.since_draft) >= DRAFT_EVERY_MS
                && let Some(open) = vad.segmenter.open_chunk(1_000)
            {
                let _ = drafts.send(open);
                vad.since_draft = 0;
            }
        }
        Ok(())
    }
}

fn spawn_writer(mut t: TrackWriter) -> JoinHandle<Result<(), hound::Error>> {
    thread::spawn(move || {
        // Recording time so far, excluding pauses, and when it was last updated.
        let mut active = t.started.elapsed();
        let mut last = Instant::now();
        loop {
            let received = t.frames.recv_timeout(Duration::from_millis(100));
            let now = Instant::now();
            let paused = t.paused.load(Ordering::Relaxed);
            if !paused {
                active += now - last;
            }
            last = now;
            match received {
                Ok(mut frame) => {
                    t.meters.frames.fetch_add(1, Ordering::Relaxed);
                    if t.muted.load(Ordering::Relaxed) {
                        frame.fill(0.0);
                    }
                    t.meters.level.store(meter(&frame).to_bits(), Ordering::Relaxed);
                    if !paused {
                        t.record(&frame)?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            // Fill a stall (or a helper restart) with silence so time stays true.
            let expected = active.as_millis() as u64 * SAMPLE_RATE as u64 / 1000;
            if !paused && expected > t.written + ms_to_samples(MAX_GAP_MS) {
                let gap = vec![0.0; (expected - t.written) as usize];
                t.meters.level.store(0f32.to_bits(), Ordering::Relaxed);
                t.record(&gap)?;
            }
        }
        if let Some(vad) = &mut t.vad
            && let Some(chunk) = vad.segmenter.flush()
        {
            let _ = vad.senders.finals.send(chunk);
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

fn ms_to_samples(ms: u64) -> u64 {
    ms * SAMPLE_RATE as u64 / 1000
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

    struct Harness {
        tx: Sender<Vec<f32>>,
        muted: Arc<AtomicBool>,
        chunks: Receiver<SpeechChunk>,
        writer: JoinHandle<Result<(), hound::Error>>,
        path: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn harness() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        let (tx, frames) = mpsc::channel();
        let (finals, chunks) = mpsc::channel();
        let muted = Arc::new(AtomicBool::new(false));
        let writer = spawn_writer(TrackWriter {
            frames,
            wav: hound::WavWriter::create(&path, wav_spec()).unwrap(),
            started: Instant::now(),
            paused: Arc::new(AtomicBool::new(false)),
            muted: muted.clone(),
            meters: Arc::new(Meters::default()),
            vad: Some(Vad {
                segmenter: Segmenter::new(Channel::Mic),
                senders: ChunkSenders { finals, drafts: None },
                since_draft: 0,
            }),
            written: 0,
        });
        Harness {
            tx,
            muted,
            chunks,
            writer,
            path,
            _dir: dir,
        }
    }

    fn tone(ms: u64) -> Vec<f32> {
        (0..ms_to_samples(ms))
            .map(|i| (i as f32 * 0.2).sin() * 0.3)
            .collect()
    }

    #[test]
    fn a_stalled_helper_is_filled_with_silence() {
        let h = harness();
        let started = Instant::now();
        h.tx.send(tone(200)).unwrap();
        // The helper stops delivering (a device switch) for over a second.
        thread::sleep(Duration::from_millis(1_300));
        h.tx.send(tone(200)).unwrap();
        drop(h.tx);
        h.writer.join().unwrap().unwrap();
        let elapsed = started.elapsed().as_millis() as u64;
        let recorded = samples_to_ms(hound::WavReader::open(&h.path).unwrap().len() as u64);
        assert!(
            recorded + MAX_GAP_MS + 100 >= elapsed && recorded <= elapsed + 250,
            "recorded {recorded} ms in {elapsed} ms"
        );
    }

    #[test]
    fn a_muted_mic_records_silence_and_transcribes_nothing() {
        let h = harness();
        h.muted.store(true, Ordering::Relaxed);
        for _ in 0..10 {
            h.tx.send(tone(100)).unwrap();
        }
        drop(h.tx);
        h.writer.join().unwrap().unwrap();
        let samples: Vec<f32> = hound::WavReader::open(&h.path)
            .unwrap()
            .samples::<f32>()
            .map(Result::unwrap)
            .collect();
        assert!(samples.len() >= ms_to_samples(1_000) as usize);
        assert!(samples.iter().all(|&s| s == 0.0));
        assert!(h.chunks.try_recv().is_err(), "no speech chunk while muted");
    }

    #[test]
    fn samples_convert_to_milliseconds() {
        assert_eq!(samples_to_ms(16_000), 1000);
        assert_eq!(samples_to_ms(1_600), 100);
    }
}
