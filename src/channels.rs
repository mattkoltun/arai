use std::sync::mpsc::{Receiver, Sender};

use crate::messages::{AudioChunk, TranscribedOutput};

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type TranscribedSender = Sender<TranscribedOutput>;
pub type TranscribedReceiver = Receiver<TranscribedOutput>;
