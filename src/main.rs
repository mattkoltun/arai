mod agent;
mod channels;
mod config;
mod controller;
mod logger;
mod messages;
mod recorder;
mod transcriber;
mod ui;

use std::sync::mpsc;
use std::thread;

fn main() {
    let config = match config::Config::load() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Config error: {err}");
            return;
        }
    };

    if let Err(err) = logger::init_with_config(logger::LogConfig {
        level: config.log_level,
        path: config.log_path.clone(),
    }) {
        eprintln!("Failed to init logger: {err}");
    }
    log::info!("Starting Arai");

    // Channels
    let (audio_tx, audio_rx) = mpsc::channel::<messages::AudioChunk>();
    let (transcript_tx, transcript_rx) = mpsc::channel::<messages::TranscribedOutput>();
    let (app_event_tx, app_event_rx) = mpsc::channel::<messages::AppEvent>();

    let recorder = recorder::Recorder::new(audio_tx, app_event_tx.clone());
    let transcriber = transcriber::Transcriber::new(audio_rx, transcript_tx, app_event_tx.clone());
    let agent = agent::Agent::new(app_event_tx.clone(), config.open_api_key.clone());
    let ui = ui::Ui::new(app_event_tx.clone());
    let controller = std::sync::Arc::new(controller::Controller::new(
        recorder,
        transcriber,
        transcript_rx,
        app_event_rx,
        agent,
        config.agent_instruction(),
        ui.clone(),
    ));
    let controller_shutdown = controller.clone();

    let controller_runner = controller.clone();
    let controller_handle = thread::spawn(move || controller_runner.run());

    if let Err(err) = ui.run() {
        log::error!("Failed to launch UI: {err}");
    }
    controller_shutdown.shutdown();

    let _ = controller_handle.join();
    log::info!("Arai shutdown complete");
}
