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
    let (transcript_tx, transcript_rx) = mpsc::channel::<messages::TranscribedOutput>();
    let (app_event_tx, app_event_rx) = mpsc::channel::<messages::AppEvent>();

    let recorder = recorder::Recorder::new(audio_tx, app_event_tx.clone());
    let transcriber = transcriber::Transcriber::new(audio_rx, transcript_tx, app_event_tx.clone());
    let ui = ui::MessageUi::new(app_event_tx);
    let ui_handle = ui.handle();
    let controller = std::sync::Arc::new(controller::Controller::new(
        recorder,
        transcriber,
        transcript_rx,
        app_event_rx,
        ui_handle,
    ));
    let controller_shutdown = controller.clone();

    let controller_runner = controller.clone();
    let controller_handle = thread::spawn(move || controller_runner.run());

    if let Err(err) = ui.run(controller) {
        eprintln!("Failed to launch UI: {err}");
    }
    controller_shutdown.shutdown();

    let _ = controller_handle.join();
}
