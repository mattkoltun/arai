#[derive(Debug)]
pub enum UICommand {
    StartRecording,
    StopRecording,
    Shutdown,
    SubmitMessage(String),
    ProcessMessage(String),
}

#[derive(Debug)]
pub enum AppCommand {
    StartListening,
    StopListening,
    Shutdown,
}

/// Captured audio payload with metadata.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub data: Vec<i16>,
}
