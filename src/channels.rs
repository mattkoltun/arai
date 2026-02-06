use std::sync::mpsc::{Receiver, Sender};

use crate::messages::{AppEvent, AudioChunk};

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type AppEventSender = Sender<AppEvent>;
pub type AppEventReceiver = Receiver<AppEvent>;
