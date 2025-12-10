use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::fmt;
use std::sync::mpsc::Sender;
use std::sync::Mutex;

/// Manages microphone recording backed by cpal.
pub struct Recorder {
    inner: Mutex<Option<ActiveRecording>>,
}

struct ActiveRecording {
    _stream: Stream,
    _sender: Sender<AudioChunk>,
}

/// Captured audio payload with metadata.
#[derive(Clone)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub data: Vec<i16>,
}

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
            RecorderError::PoisonedLock => write!(f, "recorder lock poisoned"),
        }
    }
}

impl std::error::Error for RecorderError {}

impl Recorder {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Start recording from the default input device, streaming chunks into the provided sender.
    pub fn start(&self, sink: Sender<AudioChunk>) -> Result<(), RecorderError> {
        let mut inner = self.inner.lock().map_err(|_| RecorderError::PoisonedLock)?;
        if inner.is_some() {
            return Err(RecorderError::AlreadyRecording);
        }

        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(RecorderError::NoInputDevice)?;
        let input_config = device
            .default_input_config()
            .map_err(RecorderError::StreamConfig)?;
        let sample_format = input_config.sample_format();
        let stream_config: cpal::StreamConfig = input_config.config();

        let err_fn = |err| eprintln!("input stream error: {err}");
        let stream_sender = sink.clone();
        let stream = build_stream(
            device,
            &stream_config,
            sample_format,
            stream_sender,
            err_fn,
        )?;
        if let Err(err) = stream.play() {
            drop(stream);
            drop(sink);
            return Err(RecorderError::PlayStream(err));
        }

        *inner = Some(ActiveRecording {
            _stream: stream,
            _sender: sink,
        });

        Ok(())
    }

    /// Stop the active recording and release the stream.
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), RecorderError> {
        let mut inner = self.inner.lock().map_err(|_| RecorderError::PoisonedLock)?;
        let Some(active) = inner.take() else {
            return Err(RecorderError::NotRecording);
        };

        // Dropping the stream stops callbacks; dropping the sender closes the channel.
        let ActiveRecording {
            _stream,
            _sender,
        } = active;

        Ok(())
    }
}

fn build_stream(
    device: cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    sender: Sender<AudioChunk>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, RecorderError> {
    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| send_samples_f32(&sender, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| send_samples_i16(&sender, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| send_samples_u16(&sender, data, sample_rate, channels),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        _ => Err(RecorderError::BuildStream(
            cpal::BuildStreamError::StreamConfigNotSupported,
        )),
    }
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
