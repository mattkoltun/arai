use std::sync::mpsc::{Receiver, Sender};

use crate::messages::{AudioChunk, TranscribedOutput, UiCommand};

pub type UiCommandSender = Sender<UiCommand>;
pub type UiCommandReceiver = Receiver<UiCommand>;

pub type AudioSender = Sender<AudioChunk>;
pub type AudioReceiver = Receiver<AudioChunk>;

pub type TranscribedSender = Sender<TranscribedOutput>;
pub type TranscribedReceiver = Receiver<TranscribedOutput>;
