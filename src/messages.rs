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
    /// Snapshot of config state (prompts, transcriber settings).
    ConfigSnapshot {
        agent_prompts: Vec<AgentPrompt>,
        default_prompt: usize,
        transcriber: TranscriberConfig,
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
    Stopped,
    Error(String),
    Transcription(String),
    UiStartListening,
    UiStopListening,
    UiSubmitText(String),
    UiUpdateText(String),
    UiShutdown,
    AgentResponse(String),
    UiUpdatePrompts {
        prompts: Vec<AgentPrompt>,
        default_prompt: usize,
    },
    UiUpdateTranscriber(TranscriberConfig),
}

// #[derive(Clone, Debug)]
pub struct AppEvent {
    pub source: AppEventSource,
    pub kind: AppEventKind,
}
