use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

/// Manages microphone recording backed by cpal.
pub struct Recorder {
    inner: Mutex<Option<ActiveRecording>>,
}

struct ActiveRecording {
    stream: Stream,
    sender: Sender<Vec<i16>>,
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

impl Recorder {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Start recording from the default input device, streaming chunks into the provided sender.
    pub fn start(&self, sink: Sender<Vec<i16>>) -> Result<(), RecorderError> {
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
        let stream = build_stream(device, &stream_config, sample_format, stream_sender, err_fn)?;
        if let Err(err) = stream.play() {
            drop(stream);
            drop(sink);
            return Err(RecorderError::PlayStream(err));
        }

        *inner = Some(ActiveRecording {
            stream,
            sender: sink,
        });

        Ok(())
    }

    /// Stop the active recording and release the stream.
    pub fn stop(&self) -> Result<(), RecorderError> {
        let mut inner = self.inner.lock().map_err(|_| RecorderError::PoisonedLock)?;
        let Some(active) = inner.take() else {
            return Err(RecorderError::NotRecording);
        };

        // Dropping the stream stops callbacks; dropping the sender closes the channel.
        let ActiveRecording {
            stream,
            sender,
        } = active;
        drop(stream);
        drop(sender);

        Ok(())
    }
}

fn build_stream(
    device: cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    sender: Sender<Vec<i16>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, RecorderError> {
    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| send_samples_f32(&sender, data),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| send_samples_i16(&sender, data),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| send_samples_u16(&sender, data),
                err_fn,
                None,
            )
            .map_err(RecorderError::BuildStream),
        _ => Err(RecorderError::BuildStream(
            cpal::BuildStreamError::StreamConfigNotSupported,
        )),
    }
}

fn send_samples_i16(sender: &Sender<Vec<i16>>, data: &[i16]) {
    let buffer = data.to_vec();
    let _ = sender.send(buffer);
}

fn send_samples_f32(sender: &Sender<Vec<i16>>, data: &[f32]) {
    let buffer: Vec<i16> = data
        .iter()
        .map(|&sample| {
            let scaled = sample.clamp(-1.0, 1.0) * i16::MAX as f32;
            scaled as i16
        })
        .collect();
    let _ = sender.send(buffer);
}

fn send_samples_u16(sender: &Sender<Vec<i16>>, data: &[u16]) {
    let buffer: Vec<i16> = data
        .iter()
        .map(|&sample| {
            let shifted = sample as i32 - i16::MAX as i32 - 1;
            shifted.clamp(i16::MIN as i32, i16::MAX as i32) as i16
        })
        .collect();
    let _ = sender.send(buffer);
}
