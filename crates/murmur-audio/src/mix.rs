//! Plays a session back as one track: the mic and system-audio files mixed into a
//! 16-bit WAV that's built on the fly, a byte range at a time, so seeking an hour-long
//! meeting never reads more than the part being played.

use std::path::Path;

use crate::{AudioError, SAMPLE_RATE};

const HEADER_BYTES: u64 = 44;

/// A WAV of both tracks mixed, served by byte range.
pub struct MixedWav {
    tracks: Vec<hound::WavReader<std::io::BufReader<std::fs::File>>>,
    samples: u64,
}

impl MixedWav {
    /// Opens `mic.wav` and `system.wav` in `dir`; either may be missing, not both.
    pub fn open(dir: &Path) -> Result<Self, AudioError> {
        let mut tracks = Vec::new();
        for name in ["mic.wav", "system.wav"] {
            let path = dir.join(name);
            if path.exists() {
                tracks.push(hound::WavReader::open(path)?);
            }
        }
        if tracks.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no recordings").into());
        }
        let samples = tracks.iter().map(|t| t.duration() as u64).max().unwrap_or(0);
        Ok(MixedWav { tracks, samples })
    }

    /// The size of the whole mixed file in bytes.
    pub fn len(&self) -> u64 {
        HEADER_BYTES + self.samples * 2
    }

    pub fn is_empty(&self) -> bool {
        self.samples == 0
    }

    /// Bytes `start..end` (end exclusive, clamped to the file) of the mixed file.
    pub fn read(&mut self, start: u64, end: u64) -> Result<Vec<u8>, AudioError> {
        let end = end.min(self.len());
        if start >= end {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity((end - start) as usize);
        if start < HEADER_BYTES {
            out.extend_from_slice(&header(self.samples)[start as usize..end.min(HEADER_BYTES) as usize]);
        }
        let data_start = start.max(HEADER_BYTES) - HEADER_BYTES;
        let data_end = end - HEADER_BYTES.min(end);
        if data_start < data_end {
            let first = data_start / 2;
            let last = data_end.div_ceil(2);
            let pcm = self.mix(first, last - first)?;
            let skip = (data_start - first * 2) as usize;
            out.extend_from_slice(&pcm[skip..skip + (data_end - data_start) as usize]);
        }
        Ok(out)
    }

    /// `count` mixed samples from `first`, as little-endian 16-bit PCM.
    fn mix(&mut self, first: u64, count: u64) -> Result<Vec<u8>, AudioError> {
        let mut sum = vec![0f32; count as usize];
        for track in &mut self.tracks {
            if first >= track.duration() as u64 {
                continue;
            }
            track.seek(first as u32)?;
            for (slot, sample) in sum.iter_mut().zip(track.samples::<f32>()) {
                *slot += sample?;
            }
        }
        // Each track is normalized on its own and people rarely talk over each other,
        // so a plain sum is loud enough; clamping only catches the overlaps.
        Ok(sum
            .into_iter()
            .flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes())
            .collect())
    }
}

fn header(samples: u64) -> [u8; HEADER_BYTES as usize] {
    let data = (samples * 2).min(u32::MAX as u64 - HEADER_BYTES) as u32;
    let mut h = [0u8; HEADER_BYTES as usize];
    let mut put = |at: usize, bytes: &[u8]| h[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"RIFF");
    put(4, &(data + 36).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16u32.to_le_bytes());
    put(20, &1u16.to_le_bytes()); // PCM
    put(22, &1u16.to_le_bytes()); // mono
    put(24, &SAMPLE_RATE.to_le_bytes());
    put(28, &(SAMPLE_RATE * 2).to_le_bytes());
    put(32, &2u16.to_le_bytes());
    put(34, &16u16.to_le_bytes());
    put(36, b"data");
    put(40, &data.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn whole(mixed: &mut MixedWav) -> Vec<u8> {
        let len = mixed.len();
        mixed.read(0, len).unwrap()
    }

    #[test]
    fn mixes_both_tracks_into_a_valid_wav() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("mic.wav"), &[0.25, 0.25]);
        write(&dir.path().join("system.wav"), &[0.25, -0.5, 0.9]);
        let mut mixed = MixedWav::open(dir.path()).unwrap();
        assert_eq!(mixed.len(), 44 + 6);

        let bytes = whole(&mut mixed);
        let path = dir.path().join("out.wav");
        std::fs::write(&path, &bytes).unwrap();
        let mut reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        let max = i16::MAX as f32;
        assert_eq!(
            samples,
            vec![(0.5 * max) as i16, (-0.25 * max) as i16, (0.9 * max) as i16]
        );
    }

    #[test]
    fn any_byte_range_matches_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let tone: Vec<f32> = (0..500).map(|i| (i as f32 / 20.0).sin() * 0.6).collect();
        write(&dir.path().join("system.wav"), &tone);
        let mut mixed = MixedWav::open(dir.path()).unwrap();
        let all = whole(&mut mixed);
        for (start, end) in [(0, 1), (3, 50), (43, 45), (45, 46), (101, 333), (900, 2000)] {
            let end_clamped = (end as usize).min(all.len());
            assert_eq!(
                mixed.read(start, end).unwrap(),
                all[start as usize..end_clamped],
                "{start}..{end}"
            );
        }
    }

    #[test]
    fn a_session_without_recordings_has_nothing_to_play() {
        let dir = tempfile::tempdir().unwrap();
        assert!(MixedWav::open(dir.path()).is_err());
    }
}
