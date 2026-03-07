use crate::channels::{AppEventSender, AudioReceiver};
use crate::config::TranscriberConfig;
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk};
use log::{debug, error, info};
use std::fmt;
use std::thread::{self, JoinHandle};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

/// Target sample rate required by the Whisper model.
const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug)]
pub enum TranscriberError {
    ModelNotFound(String),
}

impl fmt::Display for TranscriberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelNotFound(path) => write!(f, "Whisper model not found: {path}"),
        }
    }
}

/// Manages a background thread that receives audio chunks, resamples them,
/// and runs Whisper transcription in a sliding-window loop.
pub struct Transcriber {
    audio_rx: Option<AudioReceiver>,
    app_event_tx: AppEventSender,
    config: TranscriberConfig,
    handle: Option<JoinHandle<()>>,
}

impl Transcriber {
    /// Creates a new Transcriber without starting the worker thread. Call
    /// [`start()`](Self::start) to validate the model and begin processing.
    pub fn new(
        audio_rx: AudioReceiver,
        app_event_tx: AppEventSender,
        config: TranscriberConfig,
    ) -> Self {
        Self {
            audio_rx: Some(audio_rx),
            app_event_tx,
            config,
            handle: None,
        }
    }

    /// Validates the Whisper model path and spawns the worker thread. Returns
    /// an error if the model file does not exist.
    pub fn start(&mut self) -> Result<(), TranscriberError> {
        if !std::path::Path::new(&self.config.model_path).exists() {
            return Err(TranscriberError::ModelNotFound(
                self.config.model_path.clone(),
            ));
        }
        let audio_rx = self.audio_rx.take().expect("start() called only once");
        let app_event_tx = self.app_event_tx.clone();
        let config = self.config.clone();
        self.handle = Some(thread::spawn(move || {
            worker(audio_rx, app_event_tx, config)
        }));
        Ok(())
    }
}

/// Joins the worker thread on drop to ensure clean shutdown.
impl Drop for Transcriber {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Main transcriber loop. Loads the Whisper model, then continuously receives
/// audio chunks, accumulates them into a buffer, and runs transcription when
/// the buffer reaches the configured window size or a final chunk is received.
/// Transcription results are sent to the controller via `app_event_tx`.
fn worker(audio_rx: AudioReceiver, app_event_tx: AppEventSender, config: TranscriberConfig) {
    let ctx = match WhisperContext::new_with_params(
        &config.model_path,
        WhisperContextParameters::default(),
    ) {
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
        let window_samples = (TARGET_SAMPLE_RATE as f32 * config.window_seconds) as usize;
        let overlap_samples = (TARGET_SAMPLE_RATE as f32 * config.overlap_seconds) as usize;
        if buffer.len() >= window_samples || chunk.is_final {
            let energy = rms_energy(&buffer);
            debug!(
                "Energy gate: rms={:.6}, threshold={}, buffer_samples={}, is_final={}",
                energy,
                config.silence_threshold,
                buffer.len(),
                chunk.is_final
            );
            if energy < config.silence_threshold {
                debug!("Audio below silence threshold, skipping transcription");
                buffer.clear();
                continue;
            }
            match transcribe_audio(&ctx, &buffer) {
                Ok(text) => {
                    if !text.is_empty() {
                        debug!("Transcription result: {}", text);
                        let _ = app_event_tx.send(AppEvent {
                            source: AppEventSource::Transcriber,
                            kind: AppEventKind::Transcription(text),
                        });
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

            if chunk.is_final || overlap_samples == 0 || buffer.len() <= overlap_samples {
                buffer.clear();
            } else {
                let start = buffer.len() - overlap_samples;
                buffer.drain(0..start);
            }
        }
    }
}

/// Runs Whisper inference on a buffer of 16kHz mono f32 audio samples.
/// Uses greedy decoding with parameters tuned to reduce hallucinations:
/// `no_context` prevents decoder feedback loops, `single_segment` suits
/// short streaming windows, and `temperature_inc(0.0)` disables fallback
/// temperature increases that produce random output.
fn transcribe_audio(ctx: &WhisperContext, audio: &[f32]) -> Result<String, WhisperError> {
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_n_threads(num_cpus());
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_print_special(false);
    params.set_suppress_blank(true);
    params.set_suppress_non_speech_tokens(true);
    params.set_no_context(true);
    params.set_single_segment(true);
    params.set_temperature_inc(0.0);

    state.full(params, audio)?;
    collect_segments(&state)
}

/// Extracts and concatenates all text segments from a completed Whisper state.
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

/// Returns the number of available CPU cores as an i32 for Whisper thread config.
fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(1)
}

/// Computes the root mean square energy of an audio buffer.
/// Returns 0.0 for empty input.
fn rms_energy(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = audio.iter().map(|&s| s * s).sum();
    (sum_sq / audio.len() as f32).sqrt()
}

/// Converts an `AudioChunk` (arbitrary sample rate, mono or multi-channel, i16)
/// into mono f32 samples at 16kHz using linear interpolation resampling.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(samples: Vec<i16>, channels: u16, sample_rate: u32) -> AudioChunk {
        AudioChunk {
            samples,
            channels,
            sample_rate,
            is_final: false,
        }
    }

    #[test]
    fn converts_stereo_to_mono_average() {
        let audio = chunk(vec![32767, -32767, 32767, 32767], 2, TARGET_SAMPLE_RATE);
        let mono = resample_to_mono_16k(&audio);

        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.0).abs() < 0.01);
        assert!((mono[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn resamples_to_target_sample_rate() {
        let audio = chunk(vec![0, 16384, 32767, 0], 1, 8_000);
        let output = resample_to_mono_16k(&audio);

        assert_eq!(output.len(), 8);
    }

    #[test]
    fn empty_input_returns_empty_output() {
        let audio = chunk(vec![], 1, 48_000);
        let output = resample_to_mono_16k(&audio);
        assert!(output.is_empty());
    }

    #[test]
    fn rms_energy_returns_zero_for_empty_audio() {
        assert_eq!(rms_energy(&[]), 0.0);
    }

    #[test]
    fn rms_energy_returns_zero_for_silence() {
        let silence = vec![0.0f32; 1600];
        assert_eq!(rms_energy(&silence), 0.0);
    }

    #[test]
    fn rms_energy_detects_loud_audio() {
        let loud = vec![0.5f32; 1600];
        assert!(rms_energy(&loud) > 0.1);
    }

    #[test]
    fn rms_energy_low_for_quiet_audio() {
        let quiet = vec![0.001f32; 1600];
        assert!(rms_energy(&quiet) < 0.01);
    }
}
