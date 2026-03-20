use crate::channels::{AppEventSender, AudioSender};
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use log::{debug, error, info, warn};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Directory under the user's cache folder where recordings are stored.
const RECORDINGS_DIR: &str = ".cache/arai/recordings";

#[derive(Debug)]
pub enum RecorderError {
    AlreadyRunning,
}

pub struct Recorder {
    audio_tx: AudioSender,
    app_event_tx: AppEventSender,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    input_device: Option<String>,
}

impl Recorder {
    pub fn new(
        audio_tx: AudioSender,
        app_event_tx: AppEventSender,
        input_device: Option<String>,
    ) -> Self {
        Self {
            audio_tx,
            app_event_tx,
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
            input_device,
        }
    }

    pub fn start(&mut self) -> Result<(), RecorderError> {
        if self.handle.is_some() {
            return Err(RecorderError::AlreadyRunning);
        }

        info!("Recorder starting");
        let stop_flag = Arc::clone(&self.stop_flag);
        stop_flag.store(false, Ordering::SeqCst);
        let audio_tx = self.audio_tx.clone();
        let app_event_tx = self.app_event_tx.clone();
        let input_device = self.input_device.clone();

        let handle = thread::spawn(move || {
            let host = cpal::default_host();
            let device = match Self::find_device(&host, input_device.as_deref()) {
                Ok(d) => d,
                Err(err) => {
                    error!("{err}");
                    let _ = app_event_tx.send(AppEvent {
                        source: AppEventSource::Recorder,
                        kind: AppEventKind::Error(err),
                    });
                    return;
                }
            };
            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(err) => {
                    error!("Stream config error: {err}");
                    let _ = app_event_tx.send(AppEvent {
                        source: AppEventSource::Recorder,
                        kind: AppEventKind::Error(format!("Stream config error: {err}")),
                    });
                    return;
                }
            };
            let sample_rate = config.sample_rate();
            let channels = config.channels();
            let stream_config: cpal::StreamConfig = config.clone().into();
            let audio_tx_final = audio_tx.clone();

            // Accumulator channel: each callback sends its i16 samples here so
            // we can write the full recording to a WAV file after stopping.
            let (accum_tx, accum_rx) = std::sync::mpsc::channel::<Vec<i16>>();
            let accumulated_rx = Some(accum_rx);

            let app_event_tx_clone = app_event_tx.clone();
            let err_fn = move |err| {
                error!("Input stream error: {err}");
                let _ = app_event_tx_clone.send(AppEvent {
                    source: AppEventSource::Recorder,
                    kind: AppEventKind::Error(format!("Input stream error: {err}")),
                });
            };

            let stream_result: Result<Stream, _> = match config.sample_format() {
                SampleFormat::F32 => {
                    let accum = accum_tx.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _| {
                            let chunk: Vec<i16> = data
                                .iter()
                                .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                                .collect();
                            let _ = accum.send(chunk.clone());
                            let _ = audio_tx.send(AudioChunk {
                                sample_rate,
                                channels,
                                samples: chunk,
                                is_final: false,
                            });
                        },
                        err_fn,
                        None,
                    )
                }
                SampleFormat::I16 => {
                    let accum = accum_tx.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _| {
                            let samples = data.to_vec();
                            let _ = accum.send(samples.clone());
                            let _ = audio_tx.send(AudioChunk {
                                sample_rate,
                                channels,
                                samples,
                                is_final: false,
                            });
                        },
                        err_fn,
                        None,
                    )
                }
                SampleFormat::U16 => {
                    let accum = accum_tx.clone();
                    device.build_input_stream(
                        &stream_config,
                        move |data: &[u16], _| {
                            let chunk: Vec<i16> = data
                                .iter()
                                .map(|&s| {
                                    let shifted = s as i32 - i16::MAX as i32 - 1;
                                    shifted.clamp(i16::MIN as i32, i16::MAX as i32) as i16
                                })
                                .collect();
                            let _ = accum.send(chunk.clone());
                            let _ = audio_tx.send(AudioChunk {
                                sample_rate,
                                channels,
                                samples: chunk,
                                is_final: false,
                            });
                        },
                        err_fn,
                        None,
                    )
                }
                _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
            };
            drop(accum_tx);

            let stream = match stream_result {
                Ok(s) => s,
                Err(err) => {
                    error!("Build stream error: {err}");
                    let _ = app_event_tx.send(AppEvent {
                        source: AppEventSource::Recorder,
                        kind: AppEventKind::Error(format!("Build stream error: {err}")),
                    });
                    return;
                }
            };

            if let Err(err) = stream.play() {
                error!("Play stream error: {err}");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Recorder,
                    kind: AppEventKind::Error(format!("Play stream error: {err}")),
                });
                return;
            }

            info!("Recorder stream started");
            while !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(20));
            }

            debug!("Recorder stopping stream");
            drop(stream);

            debug!("Recorder sending final marker");
            let _ = audio_tx_final.send(AudioChunk {
                sample_rate,
                channels,
                samples: Vec::new(),
                is_final: true,
            });

            // Read back accumulated samples from the receiver and save to WAV.
            // The audio_tx_accum sender was collecting samples in the callbacks;
            // here we drain them from the dedicated accumulator channel.
            let wav_path = if let Some(ref accum) = accumulated_rx {
                let all_samples: Vec<i16> = accum.try_iter().flatten().collect();
                if all_samples.is_empty() {
                    None
                } else {
                    write_wav(&all_samples, sample_rate, channels)
                }
            } else {
                None
            };

            let _ = app_event_tx.send(AppEvent {
                source: AppEventSource::Recorder,
                kind: AppEventKind::Stopped(wav_path.map(|p| p.to_string_lossy().into_owned())),
            });
            info!("Recorder stopped");
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Returns the names of all available input devices.
    pub fn list_input_devices() -> Vec<String> {
        let host = cpal::default_host();
        let Ok(devices) = host.input_devices() else {
            return Vec::new();
        };
        devices
            .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
            .collect()
    }

    /// Updates the input device used for future recordings.
    pub fn set_input_device(&mut self, device: Option<String>) {
        self.input_device = device;
    }

    /// Finds the input device matching the configured name, or falls back to
    /// the system default. Uses exact matching against device names from the
    /// system device list.
    fn find_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device, String> {
        if let Some(wanted) = name {
            let devices = host
                .input_devices()
                .map_err(|e| format!("Failed to list input devices: {e}"))?;
            for device in devices {
                if let Ok(desc) = device.description()
                    && desc.name() == wanted
                {
                    info!("Using input device: {}", desc.name());
                    return Ok(device);
                }
            }
            return Err(format!(
                "Input device '{wanted}' not found. \
                 Remove input_device from config to use the system default."
            ));
        }
        host.default_input_device()
            .ok_or_else(|| "No input device available".to_string())
    }

    /// Signals the recorder thread to stop without blocking. The thread will
    /// send a `Recorder::Stopped` event when it finishes.
    pub fn stop_signal(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Joins the recorder thread handle, blocking until it finishes. Call this
    /// after receiving the `Stopped` event or during shutdown.
    pub fn join_handle(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Signals the recorder thread to stop and blocks until it finishes. Use
    /// this during application shutdown when blocking is acceptable.
    pub fn stop(&mut self) {
        self.stop_signal();
        self.join_handle();
    }
}

/// Returns the recordings directory path (`~/.cache/arai/recordings/`).
fn recordings_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(RECORDINGS_DIR))
}

/// Writes accumulated i16 PCM samples to a WAV file and returns the path.
/// Returns `None` if the directory cannot be created or the write fails.
fn write_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Option<std::path::PathBuf> {
    let dir = recordings_dir()?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("Failed to create recordings dir: {e}");
        return None;
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("{timestamp}.wav"));

    match write_wav_file(&path, samples, sample_rate, channels) {
        Ok(()) => {
            info!("Recording saved to {}", path.display());
            Some(path)
        }
        Err(e) => {
            warn!("Failed to write WAV: {e}");
            None
        }
    }
}

/// Writes a minimal WAV (PCM 16-bit) file. No external crate needed.
fn write_wav_file(
    path: &std::path::Path,
    samples: &[i16],
    sample_rate: u32,
    channels: u16,
) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;

    let mut f = std::fs::File::create(path)?;
    // RIFF header
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    // fmt chunk
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM format
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    // data chunk
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for &s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()
}
