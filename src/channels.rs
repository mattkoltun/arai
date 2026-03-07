use std::sync::mpsc::{Receiver, Sender};

use crate::agent::AgentRequest;
use crate::messages::{AppEvent, AudioChunk, UiUpdate};

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type AppEventSender = Sender<AppEvent>;
pub type AppEventReceiver = Receiver<AppEvent>;

pub type UiUpdateSender = Sender<UiUpdate>;
pub type UiUpdateReceiver = Receiver<UiUpdate>;

pub type AgentSender = Sender<AgentRequest>;
