//! Downloads the speech and speaker models the first time Murmur runs, so the app
//! works without the developer script (`scripts/download-models.sh`).

use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

struct ModelFile {
    label: &'static str,
    url: &'static str,
    /// Under the models folder; present means installed.
    path: &'static str,
    /// Download size, for progress when the server doesn't say.
    bytes: u64,
    /// A .tar.bz2 unpacked into this folder (under models) instead of saved as is.
    unpack_into: Option<&'static str>,
}

// The sherpa-onnx release tag really is spelled "recongition".
const FILES: [ModelFile; 4] = [
    ModelFile {
        label: "Whisper large-v3 turbo (transcription)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        path: "ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        unpack_into: None,
    },
    ModelFile {
        label: "Whisper base (drafts and languages)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        path: "ggml-base.bin",
        bytes: 147_951_465,
        unpack_into: None,
    },
    ModelFile {
        label: "TitaNet (voices)",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx",
        path: "speakers/nemo_en_titanet_small.onnx",
        bytes: 40_257_283,
        unpack_into: None,
    },
    ModelFile {
        label: "pyannote segmentation (who spoke when)",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
        path: "speakers/sherpa-onnx-pyannote-segmentation-3-0/model.onnx",
        bytes: 6_958_444,
        unpack_into: Some("speakers"),
    },
];

/// One download at a time.
#[derive(Default)]
pub struct DownloadState(AtomicBool);

#[derive(Serialize)]
pub struct MissingModel {
    label: &'static str,
    bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadEvent {
    /// `done` of `total` bytes across every missing model; `label` is downloading.
    Progress {
        label: &'static str,
        done: u64,
        total: u64,
    },
    Finished,
}

/// Models that still need downloading; empty once Murmur is ready.
#[tauri::command]
pub fn missing_models(app: AppHandle) -> Result<Vec<MissingModel>, String> {
    let dir = models_dir(&app)?;
    Ok(missing(&dir)
        .map(|f| MissingModel {
            label: f.label,
            bytes: f.bytes,
        })
        .collect())
}

#[tauri::command]
pub async fn download_models(
    app: AppHandle,
    state: State<'_, DownloadState>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    if state.0.swap(true, Ordering::AcqRel) {
        return Err("the models are already downloading".into());
    }
    let result = match models_dir(&app) {
        Ok(dir) => {
            download_into(&dir, |label, done, total| {
                let _ = on_event.send(DownloadEvent::Progress { label, done, total });
            })
            .await
        }
        Err(e) => Err(e),
    };
    state.0.store(false, Ordering::Release);
    result?;
    // Load them now so the first recording starts instantly.
    crate::recording::preload_model(&app);
    let _ = on_event.send(DownloadEvent::Finished);
    Ok(())
}

/// Downloads every model missing from `dir`, reporting (label, bytes done, total).
async fn download_into(dir: &Path, mut progress: impl FnMut(&'static str, u64, u64)) -> Result<(), String> {
    let todo: Vec<&ModelFile> = missing(dir).collect();
    let total: u64 = todo.iter().map(|f| f.bytes).sum();
    let mut done = 0;
    let mut last_sent = 0;
    for file in todo {
        let part = dir.join(format!("{}.part", file.path));
        std::fs::create_dir_all(part.parent().expect("model paths have a folder"))
            .map_err(|e| e.to_string())?;
        let response = reqwest::get(file.url)
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("could not download {}: {e}", file.label))?;
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|e| format!("the download of {} broke off: {e}", file.label))?;
            out.write_all(&chunk).map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            // A message per megabyte is plenty for a progress bar.
            if done - last_sent >= 1 << 20 {
                last_sent = done;
                progress(file.label, done.min(total), total);
            }
        }
        out.flush().map_err(|e| e.to_string())?;
        drop(out);
        match file.unpack_into {
            Some(folder) => {
                unpack(&part, &dir.join(folder))?;
                std::fs::remove_file(&part).map_err(|e| e.to_string())?;
            }
            None => std::fs::rename(&part, dir.join(file.path)).map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/tar")
        .arg("xjf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("could not unpack {}", archive.display()));
    }
    Ok(())
}

fn missing(dir: &Path) -> impl Iterator<Item = &'static ModelFile> {
    FILES.iter().filter(move |f| !dir.join(f.path).is_file())
}

fn models_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_models_are_not_missing() {
        let dir = std::env::temp_dir().join(format!("murmur-models-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("speakers")).unwrap();
        assert_eq!(missing(&dir).count(), FILES.len());
        std::fs::write(dir.join("ggml-base.bin"), b"").unwrap();
        std::fs::write(dir.join("speakers/nemo_en_titanet_small.onnx"), b"").unwrap();
        let labels: Vec<_> = missing(&dir).map(|f| f.label).collect();
        assert_eq!(labels.len(), 2);
        assert!(labels[0].starts_with("Whisper large"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Downloads the two speaker models (about 47 MB) from the real servers.
    #[tokio::test]
    #[ignore = "downloads from the internet"]
    async fn downloads_and_unpacks_models() {
        let dir = std::env::temp_dir().join(format!("murmur-download-{}", std::process::id()));
        // Pretend the large Whisper models are installed so only the small ones download.
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["ggml-large-v3-turbo-q5_0.bin", "ggml-base.bin"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let mut updates = 0;
        download_into(&dir, |_, done, total| {
            assert!(done <= total);
            updates += 1;
        })
        .await
        .unwrap();
        assert_eq!(missing(&dir).count(), 0, "everything installed");
        assert!(updates > 10, "progress reported along the way");
        let titanet = std::fs::metadata(dir.join("speakers/nemo_en_titanet_small.onnx")).unwrap();
        assert_eq!(titanet.len(), 40_257_283);
        assert!(
            !dir.join("speakers/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2.part")
                .exists()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
