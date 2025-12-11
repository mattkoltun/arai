use crate::messages::UiCommand;
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use crate::channels::TranscribedReceiver;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    Active,
    ShuttingDown,
}

pub struct Controller {
    recorder: Recorder,
    transcriber: Transcriber,
    transcript_rx: TranscribedReceiver,
    ui_update_tx: Sender<String>,
    ui_cmd_rx: Receiver<UiCommand>,
    state: ControllerState,
}

#[derive(Debug)]
pub enum ControllerError {
    Recorder(crate::recorder::RecorderError),
}

impl Controller {
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        transcript_rx: TranscribedReceiver,
        ui_update_tx: Sender<String>,
        ui_cmd_rx: Receiver<UiCommand>,
    ) -> Self {
        Self {
            recorder,
            transcriber,
            transcript_rx,
            ui_update_tx,
            ui_cmd_rx,
            state: ControllerState::Active,
        }
    }

    pub fn run(mut self) {
        while self.state == ControllerState::Active {
            for cmd in self.ui_cmd_rx.try_iter() {
                match cmd {
                    UiCommand::StartListening => {
                        let _ = self.recorder.start();
                    }
                    UiCommand::StopListening => {
                        let _ = self.recorder.stop();
                    }
                    UiCommand::Shutdown => {
                        self.state = ControllerState::ShuttingDown;
                    }
                }
            }

            for line in self.transcript_rx.try_iter() {
                let _ = self.ui_update_tx.send(line.text);
            }

            thread::sleep(std::time::Duration::from_millis(10));
        }

        let _ = self.recorder.stop();
        drop(self.transcriber);
    }
}
