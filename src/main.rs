mod agent;
mod app_state;
mod channels;
mod config;
mod controller;
mod global_hotkey;
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
    let (app_event_tx, app_event_rx) = mpsc::channel::<messages::AppEvent>();
    let (ui_update_tx, ui_update_rx) = mpsc::channel::<messages::UiUpdate>();

    let recorder =
        recorder::Recorder::new(audio_tx, app_event_tx.clone(), config.input_device.clone());
    let mut transcriber =
        transcriber::Transcriber::new(audio_rx, app_event_tx.clone(), config.transcriber.clone());
    if let Err(err) = transcriber.start() {
        eprintln!("Transcriber failed to start: {err}");
        return;
    }
    let agent = agent::Agent::new(app_event_tx.clone(), config.open_api_key.clone());

    // Global hotkey must be registered on the main thread (macOS requirement).
    let hotkey_handle = global_hotkey::HotkeyHandle::register(&config.global_hotkey);

    let ui = ui::Ui::new(app_event_tx.clone(), hotkey_handle, ui_update_rx);
    let app_state = app_state::AppState::new(config);
    let (controller, shutdown_handle) = controller::Controller::new(
        recorder,
        transcriber,
        app_event_rx,
        agent,
        app_state,
        ui_update_tx,
    );

    let controller_handle = thread::spawn(move || controller.run());

    if let Err(err) = ui.run() {
        log::error!("Failed to launch UI: {err}");
    }
    shutdown_handle.shutdown();

    let _ = controller_handle.join();
    log::info!("Arai shutdown complete");
}
