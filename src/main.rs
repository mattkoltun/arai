mod controller;
mod messages;
mod recorder;
mod transcriber;
mod ui;

fn main() {
    use std::sync::mpsc;

    let controller = controller::Controller::new();

    // Message channels
    let (ui_tx, _ui_rx) = mpsc::channel::<messages::UICommand>();
    let (worker_tx, _worker_rx) = mpsc::channel::<messages::AppCommand>();
    let (audio_tx, _audio_rx) = mpsc::channel::<messages::AudioChunk>();
    let (transcript_tx, _transcript_rx) = mpsc::channel::<String>();
    let (text_tx, _text_rx) = mpsc::channel::<String>();

    // Keep senders alive (placeholders for now).
    let _ = (ui_tx, worker_tx, audio_tx, transcript_tx, text_tx);

    if let Err(err) = ui::run_chat_ui(controller) {
        eprintln!("Failed to launch UI: {err}");
    }
}
