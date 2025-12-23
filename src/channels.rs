use std::sync::mpsc::{Receiver, Sender};

use crate::messages::{AppEvent, AudioChunk, TranscribedOutput};

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type TranscribedSender = Sender<TranscribedOutput>;
pub type TranscribedReceiver = Receiver<TranscribedOutput>;

pub type AppEventSender = Sender<AppEvent>;
pub type AppEventReceiver = Receiver<AppEvent>;
