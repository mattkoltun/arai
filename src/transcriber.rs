use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperError};

const DEFAULT_MODEL_PATH: &str = "models/ggpl-small.en.bin";
const STREAM_CHUNK_SAMPLES: usize = 16_000; // ~1 second of mono PCM at 16 kHz
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
    OutputLock,
}

impl Transcriber {
    /// Load the transcriber using the default bundled model path.
    pub fn from_default_model() -> Result<Self, TranscriberError> {
        Self::new(DEFAULT_MODEL_PATH)
    }

    /// Load the transcriber from a specific model path.
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self, TranscriberError> {
        WhisperContext::new(model_path).map(|ctx| Self { ctx }).map_err(TranscriberError::ModelLoad)
    }

    /// Transcribe 16-bit PCM samples; audio is expected to be mono at 16 kHz.
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
    pub fn transcribe_to_writer<W: Write>(&self, audio: &[f32], mut writer: W) -> Result<(), TranscriberError> {
        let text = self.transcribe_pcm_f32(audio)?;
        writer.write_all(text.as_bytes()).map_err(TranscriberError::Io)?;
        writer.flush().map_err(TranscriberError::Io)
    }

    /// Convenience wrapper for i16 PCM input that writes transcription to an output stream.
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
        input: Receiver<Vec<i16>>,
        output: Arc<Mutex<String>>,
    ) -> Result<(), TranscriberError> {
        let mut window: Vec<i16> = Vec::new();
        while let Ok(chunk) = input.recv() {
            window.extend_from_slice(&chunk);
            while window.len() >= STREAM_CHUNK_SAMPLES {
                let chunk: Vec<i16> = window.drain(..STREAM_CHUNK_SAMPLES).collect();
                let text = self.transcribe_pcm_i16(&chunk)?;
                if !text.trim().is_empty() {
                    let mut out = output.lock().map_err(|_| TranscriberError::OutputLock)?;
                    if !out.is_empty() && !out.ends_with(' ') {
                        out.push(' ');
                    }
                    out.push_str(text.trim());
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

fn default_params() -> FullParams<'static> {
    FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
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
        let path = PathBuf::from(DEFAULT_MODEL_PATH);
        assert_eq!(path, PathBuf::from("models/ggpl-small.en.bin"));
    }
}
