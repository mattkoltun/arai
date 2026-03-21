use crate::channels::AppEventSender;
use crate::config::default_model_dir;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::{error, info};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Available Whisper model variants.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub struct WhisperModel {
    pub name: &'static str,
    pub file: &'static str,
    pub size_label: &'static str,
    pub description: &'static str,
}

#[allow(dead_code)]
pub const WHISPER_MODELS: &[WhisperModel] = &[
    WhisperModel {
        name: "Tiny (English)",
        file: "ggml-tiny.en.bin",
        size_label: "~75 MB",
        description: "Fastest, least accurate",
    },
    WhisperModel {
        name: "Base (English)",
        file: "ggml-base.en.bin",
        size_label: "~142 MB",
        description: "Fast, decent accuracy",
    },
    WhisperModel {
        name: "Small (English)",
        file: "ggml-small.en.bin",
        size_label: "~487 MB",
        description: "Good balance (recommended)",
    },
    WhisperModel {
        name: "Medium (English)",
        file: "ggml-medium.en.bin",
        size_label: "~1.5 GB",
        description: "High accuracy, slower",
    },
    WhisperModel {
        name: "Large",
        file: "ggml-large-v3-turbo.bin",
        size_label: "~1.5 GB",
        description: "Best accuracy, multilingual",
    },
];

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Returns the full download URL for a model file.
#[allow(dead_code)]
fn download_url(file: &str) -> String {
    format!("{HF_BASE_URL}/{file}")
}

/// Downloads a Whisper model on a background thread. Progress and
/// completion/failure events are sent via `app_event_tx`. Set `cancel_flag`
/// to `true` from another thread to abort the download. The `.part` file is
/// cleaned up on cancel.
#[allow(dead_code)]
pub fn start_download(
    model: &WhisperModel,
    app_event_tx: AppEventSender,
    cancel_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let file = model.file.to_string();
    std::thread::spawn(move || {
        if let Err(e) = run_download(&file, &app_event_tx, &cancel_flag) {
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Ui,
                    kind: AppEventKind::ModelDownloadCancelled,
                });
            } else {
                error!("Model download failed: {e}");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Ui,
                    kind: AppEventKind::ModelDownloadFailed(e),
                });
            }
        }
    })
}

#[allow(dead_code)]
fn run_download(
    file: &str,
    app_event_tx: &AppEventSender,
    cancel_flag: &AtomicBool,
) -> Result<(), String> {
    let dest_dir = default_model_dir();
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("Failed to create model directory: {e}"))?;

    let dest_path = dest_dir.join(file);
    let part_path = dest_dir.join(format!("{file}.part"));

    let url = download_url(file);
    info!("Downloading model from {url}");

    let response = reqwest::blocking::Client::new()
        .get(&url)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut reader = response;
    let mut out_file =
        std::fs::File::create(&part_path).map_err(|e| format!("Failed to create file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks
    let mut last_progress = Instant::now();

    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            drop(out_file);
            let _ = std::fs::remove_file(&part_path);
            return Err("Cancelled".to_string());
        }

        let bytes_read =
            std::io::Read::read(&mut reader, &mut buf).map_err(|e| format!("Read error: {e}"))?;

        if bytes_read == 0 {
            break;
        }

        out_file
            .write_all(&buf[..bytes_read])
            .map_err(|e| format!("Write error: {e}"))?;

        downloaded += bytes_read as u64;

        // Throttle progress updates to ~10/sec to avoid flooding the channel.
        if last_progress.elapsed().as_millis() >= 100 {
            last_progress = Instant::now();
            let _ = app_event_tx.send(AppEvent {
                source: AppEventSource::Ui,
                kind: AppEventKind::ModelDownloadProgress(downloaded, total_bytes),
            });
        }
    }

    // Send final progress update.
    let _ = app_event_tx.send(AppEvent {
        source: AppEventSource::Ui,
        kind: AppEventKind::ModelDownloadProgress(downloaded, total_bytes),
    });

    out_file.flush().map_err(|e| format!("Flush error: {e}"))?;
    drop(out_file);

    // Atomic rename from .part to final path.
    std::fs::rename(&part_path, &dest_path)
        .map_err(|e| format!("Failed to rename downloaded file: {e}"))?;

    info!("Model downloaded to {}", dest_path.display());
    let _ = app_event_tx.send(AppEvent {
        source: AppEventSource::Ui,
        kind: AppEventKind::ModelDownloadComplete(dest_path),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_models_has_five_entries() {
        assert_eq!(WHISPER_MODELS.len(), 5);
    }

    #[test]
    fn download_url_format_is_correct() {
        let url = download_url("ggml-small.en.bin");
        assert_eq!(
            url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
        );
    }

    #[test]
    fn all_models_have_bin_extension() {
        for model in WHISPER_MODELS {
            assert!(
                model.file.ends_with(".bin"),
                "model {} missing .bin extension",
                model.name
            );
        }
    }
}
