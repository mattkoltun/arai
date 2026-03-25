use crate::config::{AgentPrompt, ThemeMode, TranscriberConfig};

/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub is_final: bool,
}

/// Finalized recording payload retained in memory for reconciliation.
#[derive(Clone, Debug)]
pub struct RecordingData {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
    pub file_size_bytes: u64,
}

/// Messages sent from the Controller to the UI via a dedicated channel.
#[derive(Clone, Debug)]
pub enum UiUpdate {
    /// New transcription text arrived (full accumulated text).
    TranscriptionUpdated(String),
    /// Recorder finished finalizing the latest session.
    RecordingFinished { file_size_bytes: Option<u64> },
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
        #[allow(dead_code)]
        api_key_status: ApiKeyStatus,
        theme_mode: ThemeMode,
    },
    /// Model download progress update for the wizard.
    ModelDownloadProgress(u64, u64),
    /// Model download completed — carries the saved model path.
    ModelDownloadComplete(std::path::PathBuf),
    /// Model download failed.
    ModelDownloadFailed(String),
    /// Model download was cancelled.
    ModelDownloadCancelled,
    /// An error occurred in a component — display to user.
    ErrorOccurred(ErrorInfo),
}

/// Status of the OpenAI API key configuration.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ApiKeyStatus {
    /// Key is stored in keyring; carries masked display string (e.g., "sk-...7xQ3").
    Keyring(String),
    /// Key is set via environment variable.
    EnvVar,
    /// No key configured.
    #[default]
    NotSet,
}

/// Structured error information for display in the UI.
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    /// Which component produced the error ("Recorder", "Transcriber", "Agent").
    #[allow(dead_code)]
    pub source: String,
    /// Short summary extracted from before the first ": " in the error message.
    #[allow(dead_code)]
    pub title: String,
    /// Full error detail extracted from after the first ": ".
    #[allow(dead_code)]
    pub detail: String,
    /// Human-readable UTC timestamp (e.g., "14:32:05").
    #[allow(dead_code)]
    pub timestamp: String,
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
    /// Recorder stopped; optionally carries the in-memory audio for reconciliation.
    Stopped(Option<RecordingData>),
    Error(String),
    Transcription(String),
    /// Streaming transcriber finished processing all buffered audio chunks.
    StreamingDrained,
    /// Full-file reconciliation transcription completed.
    ReconciliationComplete(String),
    UiStartListening(String),
    UiStopListening,
    /// Submit text for processing with the given agent instruction.
    UiSubmitText {
        text: String,
        instruction: String,
    },
    UiShutdown,
    AgentResponse(String),
    UiUpdatePrompts {
        prompts: Vec<AgentPrompt>,
        default_prompt: usize,
    },
    UiUpdateTranscriber(TranscriberConfig),
    UiUpdateInputDevice(Option<String>),
    UiUpdateGlobalHotkey(String),
    UiUpdateThemeMode(ThemeMode),
    /// Update the OpenAI API key (UI → Controller).
    #[allow(dead_code)]
    UiUpdateApiKey(String),
    /// Model download progress: (bytes_downloaded, total_bytes).
    ModelDownloadProgress(u64, u64),
    /// Model download completed successfully; carries the path to the downloaded file.
    ModelDownloadComplete(std::path::PathBuf),
    /// Model download failed with an error message.
    ModelDownloadFailed(String),
    /// Model download was cancelled by the user.
    ModelDownloadCancelled,
    /// User triggered copy+hide — save text to session history.
    UiCopied {
        text: String,
        prompt: String,
    },
}

// #[derive(Clone, Debug)]
pub struct AppEvent {
    pub source: AppEventSource,
    pub kind: AppEventKind,
}
