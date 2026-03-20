use crate::config::{AgentPrompt, TranscriberConfig};

/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub is_final: bool,
}

/// Messages sent from the Controller to the UI via a dedicated channel.
#[derive(Clone, Debug)]
pub enum UiUpdate {
    /// New transcription text arrived (full accumulated text).
    TranscriptionUpdated(String),
    /// Agent finished processing — here is the polished text.
    AgentResponseReceived(String),
    /// Agent processing failed — contains the error message.
    ProcessingFailed(String),
    /// Recording stopped, reconciliation pass is starting.
    ReconciliationStarted,
    /// Reconciliation finished — full transcription from the complete recording.
    ReconciliationComplete(String),
    /// Snapshot of config state (prompts, transcriber settings).
    ConfigSnapshot {
        agent_prompts: Vec<AgentPrompt>,
        default_prompt: usize,
        transcriber: TranscriberConfig,
        selected_input_device: Option<String>,
        global_hotkey: String,
    },
}

// #[derive(Clone, Debug)]
pub enum AppEventSource {
    Recorder,
    Transcriber,
    Agent,
    Ui,
}

#[derive(Clone, Debug)]
pub enum AppEventKind {
    /// Recorder stopped; optionally carries the path to the saved WAV file.
    Stopped(Option<String>),
    Error(String),
    Transcription(String),
    /// Streaming transcriber finished processing all buffered audio chunks.
    StreamingDrained,
    /// Full-file reconciliation transcription completed.
    ReconciliationComplete(String),
    UiStartListening(String),
    UiStopListening,
    UiSubmitText(String),
    UiShutdown,
    AgentResponse(String),
    UiUpdatePrompts {
        prompts: Vec<AgentPrompt>,
        default_prompt: usize,
    },
    UiUpdateTranscriber(TranscriberConfig),
    UiUpdateInputDevice(Option<String>),
    UiUpdateGlobalHotkey(String),
}

// #[derive(Clone, Debug)]
pub struct AppEvent {
    pub source: AppEventSource,
    pub kind: AppEventKind,
}
