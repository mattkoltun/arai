use crate::config::{AgentPrompt, TranscriberConfig};

/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub is_final: bool,
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
