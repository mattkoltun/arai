use crate::messages::{AudioChunk, WorkerCommand};
use crate::recorder::{Recorder};
use crate::transcriber::{Transcriber, TranscriberError};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    Active,
    ShuttingDown,
}

pub struct Controller {
    recorder: Recorder,
    listening: bool,
    audio_tx: Option<Sender<AudioChunk>>,
    transcriber_handle: Option<JoinHandle<()>>,
    state: ControllerState,
    _worker_rx: Receiver<WorkerCommand>,
}

#[derive(Debug)]
pub enum ControllerError {
    AlreadyListening,
    NotListening,
    ShuttingDown,
    Recorder(crate::recorder::RecorderError),
    Transcriber(TranscriberError),
}

impl Controller {
    pub fn new() -> Self {
        let (_worker_tx, worker_rx) = mpsc::channel();
        Self {
            recorder: Recorder::new(),
            listening: false,
            audio_tx: None,
            transcriber_handle: None,
            state: ControllerState::Active,
            _worker_rx: worker_rx,
        }
    }

    /// Start recording and transcription. Returns a receiver that yields transcribed text chunks.
    pub fn start_listening(&mut self) -> Result<Receiver<String>, ControllerError> {
        if self.listening {
            return Err(ControllerError::AlreadyListening);
        }
        if self.state == ControllerState::ShuttingDown {
            return Err(ControllerError::ShuttingDown);
        }

        let transcriber =
            Transcriber::from_default_model().map_err(ControllerError::Transcriber)?;
        let (audio_tx, audio_rx) = mpsc::channel::<AudioChunk>();
        let (text_tx, text_rx) = mpsc::channel::<String>();

        let handle = thread::spawn(move || {
            let _ = transcriber.transcribe_streaming(audio_rx, text_tx);
        });

        self.recorder
            .start(audio_tx.clone())
            .map_err(ControllerError::Recorder)?;

        self.audio_tx = Some(audio_tx);
        self.transcriber_handle = Some(handle);
        self.listening = true;
        Ok(text_rx)
    }

    /// Stop active recording/transcription and join worker thread.
    pub fn stop_listening(&mut self) -> Result<(), ControllerError> {
        if !self.listening {
            return Err(ControllerError::NotListening);
        }

        // Closing the audio sender stops recorder callbacks and eventually ends transcription loop.
        self.audio_tx.take();
        if let Some(handle) = self.transcriber_handle.take() {
            let _ = handle.join();
        }
        self.listening = false;
        Ok(())
    }

    /// Transition to shutdown: stop listening and mark state.
    pub fn shutdown(&mut self) {
        if self.state == ControllerState::ShuttingDown {
            return;
        }
        let _ = self.stop_listening();
        self.state = ControllerState::ShuttingDown;
    }

    pub fn state(&self) -> ControllerState {
        self.state
    }
}
