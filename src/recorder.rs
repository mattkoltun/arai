use crate::messages::AudioChunk;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Host, SampleFormat, Stream};
use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

#[derive(Debug)]
pub enum RecorderError {
    AlreadyRecording,
    NotRecording,
    NoInputDevice,
    StreamConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    PoisonedLock,
}

impl fmt::Display for RecorderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecorderError::AlreadyRecording => write!(f, "recording already in progress"),
            RecorderError::NotRecording => write!(f, "no active recording to stop"),
            RecorderError::NoInputDevice => write!(f, "no default input device available"),
            RecorderError::StreamConfig(err) => write!(f, "input stream config error: {err}"),
            RecorderError::BuildStream(err) => write!(f, "failed to build input stream: {err}"),
            RecorderError::PlayStream(err) => write!(f, "failed to start input stream: {err}"),
            RecorderError::PoisonedLock => write!(f, "recorder channel poisoned"),
        }
    }
}

impl std::error::Error for RecorderError {}

/// Recorder runs a dedicated worker thread to manage the cpal stream and deliver audio chunks downstream.
pub struct Recorder {
    cmd_tx: Sender<RecorderCommand>,
    handle: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let handle = thread::spawn(move || worker_loop(cmd_rx));
        Self {
            cmd_tx,
            handle: Some(handle),
        }
    }

    /// Start recording from the default input device, streaming chunks into the provided sender.
    pub fn start(&self, sink: Sender<AudioChunk>) -> Result<(), RecorderError> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Start { sink, resp: resp_tx })
            .map_err(|_| RecorderError::PoisonedLock)?;
        resp_rx.recv().unwrap_or(Err(RecorderError::PoisonedLock))
    }

    /// Stop the active recording and flush pending buffers.
    pub fn stop(&self) -> Result<(), RecorderError> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.cmd_tx
            .send(RecorderCommand::Stop { resp: resp_tx })
            .map_err(|_| RecorderError::PoisonedLock)?;
        resp_rx.recv().unwrap_or(Err(RecorderError::PoisonedLock))
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(RecorderCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

enum RecorderCommand {
    Start {
        sink: Sender<AudioChunk>,
        resp: Sender<Result<(), RecorderError>>,
    },
    Stop {
        resp: Sender<Result<(), RecorderError>>,
    },
    Shutdown,
}

fn worker_loop(cmd_rx: Receiver<RecorderCommand>) {
    let host = cpal::default_host();
    let (cb_tx, cb_rx) = mpsc::channel::<AudioChunk>();
    let mut stream_handle: Option<Stream> = None;
    let mut sink: Option<Sender<AudioChunk>> = None;
    let mut listening = false;

    for cmd in cmd_rx {
        match cmd {
            RecorderCommand::Start { sink: new_sink, resp } => {
                if listening {
                    let _ = resp.send(Err(RecorderError::AlreadyRecording));
                    continue;
                }
                let result = create_stream(&host, cb_tx.clone());
                match result {
                    Ok(new_stream) => {
                        stream_handle = Some(new_stream);
                        sink = Some(new_sink);
                        listening = true;
                        let _ = resp.send(Ok(()));
                    }
                    Err(err) => {
                        let _ = resp.send(Err(err));
                    }
                }
            }
            RecorderCommand::Stop { resp } => {
                if !listening {
                    let _ = resp.send(Err(RecorderError::NotRecording));
                    continue;
                }
                // Stop stream and flush pending callbacks.
                if let Some(stream) = stream_handle.take() {
                    drop(stream);
                }
                if let Some(ref sink_ch) = sink {
                    for chunk in cb_rx.try_iter() {
                        let _ = sink_ch.send(chunk);
                    }
                }
                sink = None;
                listening = false;
                let _ = resp.send(Ok(()));
            }
            RecorderCommand::Shutdown => {
                if let Some(stream) = stream_handle.take() {
                    drop(stream);
                }
                if let Some(ref sink_ch) = sink {
                    for chunk in cb_rx.try_iter() {
                        let _ = sink_ch.send(chunk);
                    }
                }
                break;
            }
        }
    }
}

fn create_stream(host: &Host, cb_tx: Sender<AudioChunk>) -> Result<Stream, RecorderError> {
    let device = host
        .default_input_device()
        .ok_or(RecorderError::NoInputDevice)?;
    let input_config = device
        .default_input_config()
        .map_err(RecorderError::StreamConfig)?;
    let sample_format = input_config.sample_format();
    let stream_config: cpal::StreamConfig = input_config.config();
    let sample_rate = stream_config.sample_rate.0;
    let channels = stream_config.channels;

    let err_fn = |err| eprintln!("input stream error: {err}");
    let stream = match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _| send_samples_f32(&cb_tx, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream)?,
        SampleFormat::I16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[i16], _| send_samples_i16(&cb_tx, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream)?,
        SampleFormat::U16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[u16], _| send_samples_u16(&cb_tx, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream)?,
        _ => {
            return Err(RecorderError::BuildStream(
                cpal::BuildStreamError::StreamConfigNotSupported,
            ))
        }
    };

    stream.play().map_err(RecorderError::PlayStream)?;
    Ok(stream)
}

fn send_samples_i16(sender: &Sender<AudioChunk>, data: &[i16], sample_rate: u32, channels: u16) {
    let _ = sender.send(AudioChunk {
        sample_rate,
        channels,
        data: data.to_vec(),
    });
}

fn send_samples_f32(sender: &Sender<AudioChunk>, data: &[f32], sample_rate: u32, channels: u16) {
    let buffer: Vec<i16> = data
        .iter()
        .map(|&sample| {
            let scaled = sample.clamp(-1.0, 1.0) * i16::MAX as f32;
            scaled as i16
        })
        .collect();
    let _ = sender.send(AudioChunk {
        sample_rate,
        channels,
        data: buffer,
    });
}

fn send_samples_u16(sender: &Sender<AudioChunk>, data: &[u16], sample_rate: u32, channels: u16) {
    let buffer: Vec<i16> = data
        .iter()
        .map(|&sample| {
            let shifted = sample as i32 - i16::MAX as i32 - 1;
            shifted.clamp(i16::MIN as i32, i16::MAX as i32) as i16
        })
        .collect();
    let _ = sender.send(AudioChunk {
        sample_rate,
        channels,
        data: buffer,
    });
}
