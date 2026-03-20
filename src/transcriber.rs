use crate::channels::{AppEventSender, AudioReceiver};
use crate::config::TranscriberConfig;
use crate::messages::{AppEvent, AppEventKind, AppEventSource, AudioChunk};
use log::{debug, error, info, trace};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    stop_flag: Arc<AtomicBool>,
    /// When set, the worker drains buffered chunks without running Whisper
    /// inference. Used after recording stops so reconciliation can start
    /// sooner — the full audio is already saved to a WAV file.
    drain_flag: Arc<AtomicBool>,
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
            stop_flag: Arc::new(AtomicBool::new(false)),
            drain_flag: Arc::new(AtomicBool::new(false)),
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
        let stop_flag = Arc::clone(&self.stop_flag);
        let drain_flag = Arc::clone(&self.drain_flag);
        self.handle = Some(thread::spawn(move || {
            worker(audio_rx, app_event_tx, config, stop_flag, drain_flag)
        }));
        Ok(())
    }

    /// Tells the worker to drain remaining chunks without running inference.
    /// The full audio is already saved to a WAV file for reconciliation.
    pub fn drain_without_inference(&self) {
        self.drain_flag.store(true, Ordering::SeqCst);
    }

    /// Resets the drain flag so the next recording session runs normally.
    pub fn reset_drain(&self) {
        self.drain_flag.store(false, Ordering::SeqCst);
    }

    /// Signals the worker to stop processing and exit promptly.
    pub fn stop(&self) {
        self.drain_flag.store(true, Ordering::SeqCst);
        self.stop_flag.store(true, Ordering::SeqCst);
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
fn worker(
    audio_rx: AudioReceiver,
    app_event_tx: AppEventSender,
    config: TranscriberConfig,
    stop_flag: Arc<AtomicBool>,
    drain_flag: Arc<AtomicBool>,
) {
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
        if stop_flag.load(Ordering::SeqCst) {
            debug!("Transcriber stop flag set, exiting");
            break;
        }
        let is_final = chunk.is_final;

        // When drain flag is set, skip inference on buffered chunks.
        // The full audio is saved to WAV for reconciliation.
        if drain_flag.load(Ordering::Relaxed) {
            if is_final {
                info!("Streaming transcription drained (fast path)");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Transcriber,
                    kind: AppEventKind::StreamingDrained,
                });
            }
            continue;
        }

        trace!("Transcriber received audio chunk");
        buffer.extend(resample_to_mono_16k(&chunk));
        let window_samples = (TARGET_SAMPLE_RATE as f32 * config.window_seconds) as usize;
        let overlap_samples = (TARGET_SAMPLE_RATE as f32 * config.overlap_seconds) as usize;
        if buffer.len() >= window_samples || is_final {
            let energy = rms_energy(&buffer);
            debug!(
                "Energy gate: rms={:.6}, threshold={}, buffer_samples={}, is_final={}",
                energy,
                config.silence_threshold,
                buffer.len(),
                is_final
            );
            if energy < config.silence_threshold {
                debug!("Audio below silence threshold, skipping transcription");
                buffer.clear();
            } else {
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

                if is_final || overlap_samples == 0 || buffer.len() <= overlap_samples {
                    buffer.clear();
                } else {
                    let start = buffer.len() - overlap_samples;
                    buffer.drain(0..start);
                }
            }

            // Signal the controller that all buffered audio has been processed.
            if is_final {
                info!("Streaming transcription drained");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Transcriber,
                    kind: AppEventKind::StreamingDrained,
                });
            }
        }
    }
}

/// Transcribes an entire WAV file in one pass using Whisper. Reads the file,
/// resamples to 16kHz mono, and runs full inference. This is used for
/// reconciliation after a recording session to produce a clean transcript
/// from the complete audio.
pub fn transcribe_wav_file(model_path: &str, wav_path: &str) -> Result<String, String> {
    let samples = read_wav_to_f32(wav_path).map_err(|e| format!("Failed to read WAV: {e}"))?;
    if samples.is_empty() {
        return Ok(String::new());
    }
    let energy = rms_energy(&samples);
    if energy < 0.003 {
        info!("Reconciliation: audio is silence (rms={energy:.6}), skipping");
        return Ok(String::new());
    }
    info!(
        "Reconciliation: transcribing {} samples ({:.1}s), rms={:.6}",
        samples.len(),
        samples.len() as f32 / TARGET_SAMPLE_RATE as f32,
        energy,
    );
    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .map_err(|e| format!("Failed to load model: {e}"))?;
    transcribe_audio_full(&ctx, &samples).map_err(|e| format!("Transcription error: {e}"))
}

/// Reads a 16-bit PCM WAV file and returns mono f32 samples at 16kHz.
fn read_wav_to_f32(path: &str) -> Result<Vec<f32>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 12 {
        return Err("WAV file too short".to_string());
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("Not a valid WAV file".to_string());
    }

    let mut channels: Option<u16> = None;
    let mut sample_rate: Option<u32> = None;
    let mut bits_per_sample: Option<u16> = None;
    let mut pcm_data: Option<&[u8]> = None;

    // Walk RIFF sub-chunks starting after the WAVE identifier.
    let mut offset = 12;
    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let chunk_start = offset + 8;
        let chunk_end = (chunk_start + chunk_size).min(data.len());

        if chunk_id == b"fmt " && chunk_end >= chunk_start + 16 {
            let d = &data[chunk_start..chunk_end];
            channels = Some(u16::from_le_bytes([d[2], d[3]]));
            sample_rate = Some(u32::from_le_bytes([d[4], d[5], d[6], d[7]]));
            bits_per_sample = Some(u16::from_le_bytes([d[14], d[15]]));
        } else if chunk_id == b"data" {
            pcm_data = Some(&data[chunk_start..chunk_end]);
        }

        offset = chunk_start + chunk_size;
        // WAV chunks are word-aligned
        if !chunk_size.is_multiple_of(2) {
            offset += 1;
        }
    }

    let channels = channels.ok_or("No fmt chunk found")?;
    let sample_rate = sample_rate.ok_or("No fmt chunk found")?;
    let bps = bits_per_sample.ok_or("No fmt chunk found")?;
    let pcm_data = pcm_data.ok_or("No data chunk found")?;

    if bps != 16 {
        return Err(format!("Unsupported bits per sample: {bps}"));
    }

    let samples_i16: Vec<i16> = pcm_data
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();

    let chunk = AudioChunk {
        sample_rate,
        channels,
        samples: samples_i16,
        is_final: true,
    };
    Ok(resample_to_mono_16k(&chunk))
}

/// Runs Whisper inference on a full recording. Unlike [`transcribe_audio`],
/// this allows multiple segments and context carry-over for better accuracy
/// on longer audio.
fn transcribe_audio_full(ctx: &WhisperContext, audio: &[f32]) -> Result<String, WhisperError> {
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
    params.set_suppress_nst(true);
    params.set_temperature_inc(0.0);

    state.full(params, audio)?;
    collect_segments(&state)
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
    params.set_suppress_nst(true);
    params.set_no_context(true);
    params.set_single_segment(true);
    params.set_temperature_inc(0.0);

    state.full(params, audio)?;
    collect_segments(&state)
}

/// Extracts and concatenates all text segments from a completed Whisper state.
fn collect_segments(state: &whisper_rs::WhisperState) -> Result<String, WhisperError> {
    let segments = state.full_n_segments();
    debug!("Transcription segments: {}", segments);
    let mut output = String::new();
    for i in 0..segments {
        let Some(segment) = state.get_segment(i) else {
            continue;
        };
        let text = segment.to_str()?;
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(text.trim());
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
