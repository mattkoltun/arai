mod channels;
mod controller;
mod logger;
mod messages;
mod recorder;
mod transcriber;
mod ui;

use std::sync::mpsc;
use std::thread;

fn main() {
    let mut log_config = logger::LogConfig::default();
    if let Ok(level) = std::env::var("ARAI_LOG_LEVEL") && let Some(parsed) = logger::parse_level(&level) {
        log_config.level = parsed;
    }
    if let Ok(path) = std::env::var("ARAI_LOG_PATH") {
        log_config.path = path.into();
    }
    if let Err(err) = logger::init_with_config(log_config) {
        eprintln!("Failed to init logger: {err}");
    }
    log::info!("Starting Arai");

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
        log::error!("Failed to launch UI: {err}");
    }
    controller_shutdown.shutdown();

    let _ = controller_handle.join();
    log::info!("Arai shutdown complete");
}
