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
    /// Snapshot of config state (prompts, transcriber settings, audio devices).
    ConfigSnapshot {
        agent_prompts: Vec<AgentPrompt>,
        default_prompt: usize,
        transcriber: TranscriberConfig,
        input_devices: Vec<String>,
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
    Stopped,
    Error(String),
    Transcription(String),
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
