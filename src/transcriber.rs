use std::io::{self, Write};
use std::path::Path;
use std::ptr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Once;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

const DEFAULT_MODEL_PATH: &str = "models/ggml-small.en.bin";
const TARGET_SAMPLE_RATE: usize = 16_000; // Hz
const STREAM_CHUNK_SAMPLES: usize = TARGET_SAMPLE_RATE * 3; // ~3 seconds of mono PCM at 16 kHz
static SILENCE_LOG: Once = Once::new();
/// Transcribes audio chunks with a local Whisper model.
pub struct Transcriber {
    ctx: WhisperContext,
}

#[derive(Debug)]
pub enum TranscriberError {
    ModelLoad(WhisperError),
    CreateState(WhisperError),
    Run(WhisperError),
    Io(io::Error),
}

impl std::fmt::Display for TranscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriberError::ModelLoad(err) => write!(f, "failed to load model: {err}"),
            TranscriberError::CreateState(err) => write!(f, "failed to create whisper state: {err}"),
            TranscriberError::Run(err) => write!(f, "whisper run error: {err}"),
            TranscriberError::Io(err) => write!(f, "io error: {err}"),
        }
    }
}

impl std::error::Error for TranscriberError {}

impl Transcriber {
    /// Load the transcriber using the default bundled model path.
    pub fn from_default_model() -> Result<Self, TranscriberError> {
        Self::new(DEFAULT_MODEL_PATH)
    }

    /// Load the transcriber from a specific model path.
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self, TranscriberError> {
        install_silent_log();
        let path_lossy = model_path.as_ref().to_string_lossy();
        WhisperContext::new_with_params(&path_lossy, WhisperContextParameters::default())
            .map(|ctx| Self { ctx })
            .map_err(TranscriberError::ModelLoad)
    }

    /// Transcribe 16-bit PCM samples; audio is expected to be mono at 16 kHz.
    #[allow(dead_code)]
    pub fn transcribe_pcm_i16(&self, audio: &[i16]) -> Result<String, TranscriberError> {
        let normalized: Vec<f32> = audio.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
        self.transcribe_pcm_f32(&normalized)
    }

    /// Transcribe f32 PCM samples; audio is expected to be mono at 16 kHz.
    pub fn transcribe_pcm_f32(&self, audio: &[f32]) -> Result<String, TranscriberError> {
        let mut state = self.ctx.create_state().map_err(TranscriberError::CreateState)?;
        let mut params = default_params();
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_n_threads(num_cpus());

        state.full(params, audio).map_err(TranscriberError::Run)?;
        collect_segments(&state)
    }

    /// Transcribe and write the text to an output stream (stdout, file, buffer, etc.).
    #[allow(dead_code)]
    pub fn transcribe_to_writer<W: Write>(&self, audio: &[f32], mut writer: W) -> Result<(), TranscriberError> {
        let text = self.transcribe_pcm_f32(audio)?;
        writer.write_all(text.as_bytes()).map_err(TranscriberError::Io)?;
        writer.flush().map_err(TranscriberError::Io)
    }

    /// Convenience wrapper for i16 PCM input that writes transcription to an output stream.
    #[allow(dead_code)]
    pub fn transcribe_pcm_i16_to_writer<W: Write>(
        &self,
        audio: &[i16],
        writer: W,
    ) -> Result<(), TranscriberError> {
        let normalized: Vec<f32> = audio.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
        self.transcribe_to_writer(&normalized, writer)
    }

    /// Consume incoming i16 chunks, transcribe in near-real-time windows, and append text into `output`.
    /// Blocks until `input` is closed.
    pub fn transcribe_streaming(
        &self,
        input: Receiver<crate::recorder::AudioChunk>,
        output: Sender<String>,
    ) -> Result<(), TranscriberError> {
        let mut resample_cursor: f32 = 0.0;
        let mut mono_buffer: Vec<f32> = Vec::new();
        while let Ok(chunk) = input.recv() {
            let mono = downmix_to_mono(&chunk.data, chunk.channels);
            let step = (chunk.sample_rate as f32) / (TARGET_SAMPLE_RATE as f32);
            if step <= 0.0 {
                continue;
            }

            let mut idx = resample_cursor;
            while (idx as usize) < mono.len() {
                mono_buffer.push(mono[idx as usize]);
                idx += step;
            }
            resample_cursor = idx - (mono.len() as f32);

            while mono_buffer.len() >= STREAM_CHUNK_SAMPLES {
                let chunk: Vec<f32> = mono_buffer.drain(..STREAM_CHUNK_SAMPLES).collect();
                let text = self.transcribe_pcm_f32(&chunk)?;
                if !text.trim().is_empty() {
                    if output.send(text.trim().to_owned()).is_err() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

fn collect_segments(state: &whisper_rs::WhisperState) -> Result<String, TranscriberError> {
    let segments = state.full_n_segments().map_err(TranscriberError::Run)?;
    let mut output = String::new();
    for i in 0..segments {
        let segment = state.full_get_segment_text(i).map_err(TranscriberError::Run)?;
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(segment.trim());
    }
    Ok(output)
}

fn default_params() -> FullParams<'static, 'static> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_realtime(false);
    params.set_print_progress(false);
    params.set_print_timestamps(false);
    params.set_print_special(false);
    params
}

fn install_silent_log() {
    SILENCE_LOG.call_once(|| unsafe {
        whisper_rs::set_log_callback(Some(silent_log), ptr::null_mut());
    });
}

unsafe extern "C" fn silent_log(
    _level: std::os::raw::c_uint,
    _text: *const std::os::raw::c_char,
    _user_data: *mut std::os::raw::c_void,
) {
    // Suppress underlying whisper.cpp logs.
}

fn downmix_to_mono(data: &[i16], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
    }
    let mut mono = Vec::with_capacity(data.len() / channels as usize);
    for frame in data.chunks(channels as usize) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        let avg = sum as f32 / channels as f32;
        mono.push(avg / i16::MAX as f32);
    }
    mono
}

fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_exists_as_pathbuf() {
        let path = Path::new(DEFAULT_MODEL_PATH);
        assert_eq!(path, Path::new("models/ggml-small.en.bin"));
    }
}
