//! Peak normalization for the saved WAV tracks. Capture keeps the raw signal (Whisper
//! doesn't need gain), but a laptop mic peaks around −30 dBFS, which plays back quietly.

use std::path::Path;

use crate::AudioError;

/// Target peak: −1 dBFS.
const TARGET_PEAK: f32 = 0.891;
/// Caps the boost at +30 dB so a near-silent track doesn't become loud noise.
const MAX_GAIN: f32 = 31.6;

/// Rescales a mono f32 WAV so its peak sits at −1 dBFS. Streams the file twice (find
/// the peak, then rewrite through a temp file) so memory stays flat for long meetings.
/// Returns the applied gain (1.0 when nothing changed).
pub fn normalize_wav(path: &Path) -> Result<f32, AudioError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let mut peak = 0.0f32;
    for sample in reader.samples::<f32>() {
        peak = peak.max(sample?.abs());
    }
    if peak <= 0.0 {
        return Ok(1.0);
    }
    let gain = (TARGET_PEAK / peak).min(MAX_GAIN);
    // Already close to the target: skip the rewrite.
    if (0.95..=1.05).contains(&gain) {
        return Ok(1.0);
    }

    let tmp = path.with_extension("wav.tmp");
    {
        let mut reader = hound::WavReader::open(path)?;
        let mut writer = hound::WavWriter::create(&tmp, spec)?;
        for sample in reader.samples::<f32>() {
            writer.write_sample((sample? * gain).clamp(-1.0, 1.0))?;
        }
        writer.finalize()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn read(path: &Path) -> Vec<f32> {
        hound::WavReader::open(path)
            .unwrap()
            .samples::<f32>()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn boosts_quiet_audio_to_target_peak() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write(&path, &[0.1, -0.2, 0.05]);
        let gain = normalize_wav(&path).unwrap();
        assert!((gain - TARGET_PEAK / 0.2).abs() < 1e-4);
        let out = read(&path);
        assert!((out[1] + TARGET_PEAK).abs() < 1e-4);
        assert!(!path.with_extension("wav.tmp").exists());
    }

    #[test]
    fn caps_gain_and_leaves_silence_alone() {
        let dir = tempfile::tempdir().unwrap();
        let quiet = dir.path().join("q.wav");
        write(&quiet, &[0.001, -0.001]);
        assert_eq!(normalize_wav(&quiet).unwrap(), MAX_GAIN);

        let silent = dir.path().join("s.wav");
        write(&silent, &[0.0; 10]);
        assert_eq!(normalize_wav(&silent).unwrap(), 1.0);

        let empty = dir.path().join("e.wav");
        write(&empty, &[]);
        assert_eq!(normalize_wav(&empty).unwrap(), 1.0);
    }
}
