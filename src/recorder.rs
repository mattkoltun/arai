use crate::channels::{AppEventSender, AudioSender};
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use log::{debug, error, info};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug)]
pub enum RecorderError {
    AlreadyRunning,
}

pub struct Recorder {
    audio_tx: AudioSender,
    app_event_tx: AppEventSender,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn new(audio_tx: AudioSender, app_event_tx: AppEventSender) -> Self {
        Self {
            audio_tx,
            app_event_tx,
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
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

        let handle = thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    error!("No input device available");
                    let _ = app_event_tx.send(AppEvent {
                        source: AppEventSource::Recorder,
                        kind: AppEventKind::Error("No input device".into()),
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
            let sample_rate = config.sample_rate().0;
            let channels = config.channels();
            let stream_config: cpal::StreamConfig = config.clone().into();
            let last_chunk: Arc<Mutex<Option<Vec<i16>>>> = Arc::new(Mutex::new(None));
            let last_chunk_cb = Arc::clone(&last_chunk);
            let audio_tx_final = audio_tx.clone();

            let app_event_tx_clone = app_event_tx.clone();
            let err_fn = move |err| {
                error!("Input stream error: {err}");
                let _ = app_event_tx_clone.send(AppEvent {
                    source: AppEventSource::Recorder,
                    kind: AppEventKind::Error(format!("Input stream error: {err}")),
                });
            };

            let stream_result: Result<Stream, _> = match config.sample_format() {
                SampleFormat::F32 => device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        let audio_tx = audio_tx.clone();
                        let chunk: Vec<i16> = data
                            .iter()
                            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                            .collect();
                        if let Ok(mut last) = last_chunk_cb.lock() {
                            *last = Some(chunk.clone());
                        }

                        let _ = audio_tx.send(AudioChunk {
                            sample_rate,
                            channels,
                            samples: chunk,
                            is_final: false,
                        });
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::I16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        let audio_tx = audio_tx.clone();
                        let chunk = data.to_vec();
                        if let Ok(mut last) = last_chunk_cb.lock() {
                            *last = Some(chunk.clone());
                        }

                        let _ = audio_tx.send(AudioChunk {
                            sample_rate,
                            channels,
                            samples: chunk,
                            is_final: false,
                        });
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::U16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        let audio_tx = audio_tx.clone();
                        let chunk: Vec<i16> = data
                            .iter()
                            .map(|&s| {
                                let shifted = s as i32 - i16::MAX as i32 - 1;
                                shifted.clamp(i16::MIN as i32, i16::MAX as i32) as i16
                            })
                            .collect();

                        if let Ok(mut last) = last_chunk_cb.lock() {
                            *last = Some(chunk.clone());
                        }
                        let _ = audio_tx.send(AudioChunk {
                            sample_rate,
                            channels,
                            samples: chunk,
                            is_final: false,
                        });
                    },
                    err_fn,
                    None,
                ),
                _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
            };

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

            debug!("Recorder stopping");
            if let Ok(mut last) = last_chunk.lock()
                && let Some(samples) = last.take()
            {
                let _ = audio_tx_final.send(AudioChunk {
                    sample_rate,
                    channels,
                    samples,
                    is_final: true,
                });
            }

            let _ = app_event_tx.send(AppEvent {
                source: AppEventSource::Recorder,
                kind: AppEventKind::Stopped,
            });
            info!("Recorder stopped");
        });

        self.handle = Some(handle);
        Ok(())
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
