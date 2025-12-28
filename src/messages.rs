/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub is_final: bool,
}

#[derive(Clone, Debug)]
pub struct TranscribedOutput {
    pub text: String,
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
    Started,
    Stopped,
    Error(String),
    Status(String),
    UiStartListening,
    UiStopListening,
    UiSubmitText(String),
    UiShutdown,
    AgentResponse(String),
}

// #[derive(Clone, Debug)]
pub struct AppEvent {
    pub source: AppEventSource,
    pub kind: AppEventKind,
}
