use crate::channels::{AppEventSender, AudioReceiver, TranscribedSender};
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk, TranscribedOutput};
use log::{debug, error, info};
use std::thread::{self, JoinHandle};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

const MODEL_PATH: &str = "models/ggml-small.en.bin";
const TARGET_SAMPLE_RATE: u32 = 16_000;
const WINDOW_SECONDS: f32 = 2.0;
const OVERLAP_SECONDS: f32 = 0.25;

pub struct Transcriber {
    handle: Option<JoinHandle<()>>,
}

impl Transcriber {
    pub fn new(
        audio_rx: AudioReceiver,
        output_tx: TranscribedSender,
        app_event_tx: AppEventSender,
    ) -> Self {
        let handle = thread::spawn(move || worker(audio_rx, output_tx, app_event_tx));
        Self {
            handle: Some(handle),
        }
    }
}

impl Drop for Transcriber {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn worker(audio_rx: AudioReceiver, output_tx: TranscribedSender, app_event_tx: AppEventSender) {
    let ctx = match WhisperContext::new_with_params(MODEL_PATH, WhisperContextParameters::default())
    {
        Ok(c) => c,
        Err(err) => {
            error!("Failed to load model: {err}");
            let _ = app_event_tx.send(AppEvent {
                source: AppEventSource::Transcriber,
                kind: AppEventKind::Error(format!("Failed to load model: {err}")),
            });
            return;
        }
    };

    info!("Transcriber ready");
    let mut buffer = Vec::new();
    while let Ok(chunk) = audio_rx.recv() {
        debug!("Transcriber received audio chunk");
        buffer.extend(resample_to_mono_16k(&chunk));
        let window_samples = (TARGET_SAMPLE_RATE as f32 * WINDOW_SECONDS) as usize;
        let overlap_samples = (TARGET_SAMPLE_RATE as f32 * OVERLAP_SECONDS) as usize;
        if buffer.len() >= window_samples || chunk.is_final {
            match transcribe_audio(&ctx, &buffer) {
                Ok(text) => {
                    if !text.is_empty() {
                        println!("Transcribed: {}", text);
                        debug!("Transcription result: {}", text);
                        let _ = output_tx.send(TranscribedOutput { text });
                    }
                }
                Err(err) => {
                    error!("Transcription error: {err}");
                    let _ = app_event_tx.send(AppEvent {
                        source: AppEventSource::Transcriber,
                        kind: AppEventKind::Error(format!("Transcription error: {err}")),
                    });
                }
            }

            if chunk.is_final {
                buffer.clear();
            } else if overlap_samples == 0 || buffer.len() <= overlap_samples {
                buffer.clear();
            } else {
                let start = buffer.len() - overlap_samples;
                buffer.drain(0..start);
            }
        }
    }
}

fn transcribe_audio(ctx: &WhisperContext, audio: &[f32]) -> Result<String, WhisperError> {
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_n_threads(num_cpus());

    state.full(params, &audio)?;
    collect_segments(&state)
}

fn collect_segments(state: &whisper_rs::WhisperState) -> Result<String, WhisperError> {
    let segments = state.full_n_segments()?;
    debug!("Transcription segments: {}", segments);
    let mut output = String::new();
    for i in 0..segments {
        let segment = state.full_get_segment_text(i)?;
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(segment.trim());
    }
    Ok(output)
}

fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(1)
}

fn resample_to_mono_16k(chunk: &AudioChunk) -> Vec<f32> {
    let channels = chunk.channels.max(1) as usize;
    let mut mono = Vec::with_capacity(chunk.samples.len() / channels);

    if channels == 1 {
        mono.extend(chunk.samples.iter().map(|&s| s as f32 / i16::MAX as f32));
    } else {
        for frame in chunk.samples.chunks(channels) {
            let mut sum = 0.0f32;
            for &s in frame {
                sum += s as f32 / i16::MAX as f32;
            }
            mono.push(sum / channels as f32);
        }
    }

    if chunk.sample_rate == TARGET_SAMPLE_RATE {
        return mono;
    }

    let input_len = mono.len();
    if input_len == 0 {
        return Vec::new();
    }

    let ratio = TARGET_SAMPLE_RATE as f32 / chunk.sample_rate as f32;
    let output_len = (input_len as f32 * ratio).round() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f32 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = src_pos - idx as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        output.push(a + (b - a) * frac);
    }

    output
}
