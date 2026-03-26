use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

use crate::llm::LlmRequest;
use crate::messages::{AppEvent, AudioChunk, UiUpdate};

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type AppEventSender = Sender<AppEvent>;
pub type AppEventReceiver = Receiver<AppEvent>;

pub type UiUpdateSender = Sender<UiUpdate>;
pub type UiUpdateReceiver = Receiver<UiUpdate>;

pub type LlmSender = Sender<LlmRequest>;

/// Groups the application-wide event and UI update channels used at startup.
pub struct AppChannels {
    pub app_event_tx: AppEventSender,
    pub app_event_rx: AppEventReceiver,
    pub ui_update_tx: UiUpdateSender,
    pub ui_update_rx: UiUpdateReceiver,
}

impl AppChannels {
    /// Creates the channels used to communicate between the UI and controller.
    pub fn new() -> Self {
        let (app_event_tx, app_event_rx) = mpsc::channel::<AppEvent>();
        let (ui_update_tx, ui_update_rx) = mpsc::channel::<UiUpdate>();
        Self {
            app_event_tx,
            app_event_rx,
            ui_update_tx,
            ui_update_rx,
        }
    }
}

/// Groups the recorder-to-transcriber audio channel pair.
pub struct AudioChannels {
    pub audio_tx: AudioSender,
    pub audio_rx: AudioReceiver,
}

impl AudioChannels {
    /// Creates the audio channel pair used for recorder and transcriber wiring.
    pub fn new() -> Self {
        let (audio_tx, audio_rx) = mpsc::channel::<AudioChunk>();
        Self { audio_tx, audio_rx }
    }
}
