use crate::channels::{AppEventSender, AudioReceiver, TranscribedSender};
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk, TranscribedOutput};
use log::{debug, error, info};
use std::thread::{self, JoinHandle};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

const MODEL_PATH: &str = "models/ggml-small.en.bin";

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
    while let Ok(chunk) = audio_rx.recv() {
        debug!("Transcriber received audio chunk");
        match transcribe_chunk(&ctx, &chunk) {
            Ok(text) => {
                let _ = output_tx.send(TranscribedOutput { text });
            }
            Err(err) => {
                error!("Transcription error: {err}");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Transcriber,
                    kind: AppEventKind::Error(format!("Transcription error: {err}")),
                });
            }
        }
    }
}

fn transcribe_chunk(ctx: &WhisperContext, chunk: &AudioChunk) -> Result<String, WhisperError> {
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_n_threads(num_cpus());

    let audio: Vec<f32> = chunk
        .samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect();

    state.full(params, &audio)?;
    collect_segments(&state)
}

fn collect_segments(state: &whisper_rs::WhisperState) -> Result<String, WhisperError> {
    let segments = state.full_n_segments()?;
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
