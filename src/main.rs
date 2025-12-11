mod channels;
mod controller;
mod messages;
mod recorder;
mod transcriber;
mod ui;

use std::sync::mpsc;
use std::thread;

fn main() {
    // Channels
    let (audio_tx, audio_rx) = mpsc::channel::<messages::AudioChunk>();
    let (ui_cmd_tx, ui_cmd_rx) = mpsc::channel::<messages::UiCommand>();
    let (transcript_tx, transcript_rx) = mpsc::channel::<messages::TranscribedOutput>();
    let (ui_update_tx, ui_update_rx) = mpsc::channel::<String>();

    let recorder = recorder::Recorder::new(audio_tx.clone());
    let transcriber = transcriber::Transcriber::new(audio_rx, transcript_tx);
    let controller =
        controller::Controller::new(recorder, transcriber, transcript_rx, ui_update_tx, ui_cmd_rx);

    thread::spawn(move || controller.run());

    if let Err(err) = ui::run_chat_ui(ui_cmd_tx, ui_update_rx) {
        eprintln!("Failed to launch UI: {err}");
    }
}
