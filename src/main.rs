mod agent;
mod app_state;
mod channels;
mod config;
mod controller;
mod global_hotkey;
mod history;
mod keyring_store;
mod logger;
mod messages;
mod model_downloader;
mod recorder;
mod theme;
mod transcriber;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

    let model_exists = std::path::Path::new(&config.transcriber.model_path).exists();
    let api_key_exists = !config.open_api_key.is_empty();

    if let Err(err) = logger::init_with_config(logger::LogConfig {
        level: config.parsed_log_level(),
        path: config.parsed_log_path(),
    }) {
        eprintln!("Failed to init logger: {err}");
    }
    log::info!("Starting Arai");

    // Channels
    let (app_event_tx, app_event_rx) = mpsc::channel::<messages::AppEvent>();
    let (ui_update_tx, ui_update_rx) = mpsc::channel::<messages::UiUpdate>();

    // Global hotkey must be registered on the main thread (macOS requirement).
    let hotkey_handle = global_hotkey::HotkeyHandle::register(&config.global_hotkey);

    let ui = ui::Ui::new(
        app_event_tx.clone(),
        hotkey_handle,
        ui_update_rx,
        model_exists,
        api_key_exists,
    );

    // Shared shutdown flag — set by main after the UI exits so the
    // controller run loop knows to stop.
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let controller_shutdown = Arc::clone(&shutdown_flag);

    // Spawn controller and all subsystems on a background thread so the UI
    // window appears immediately without waiting for initialization.
    let controller_event_tx = app_event_tx.clone();
    let controller_handle = thread::spawn(move || {
        let (audio_tx, audio_rx) = mpsc::channel::<messages::AudioChunk>();

        let recorder = recorder::Recorder::new(
            audio_tx,
            controller_event_tx.clone(),
            config.input_device.clone(),
        );
        let mut transcriber = transcriber::Transcriber::new(
            audio_rx,
            controller_event_tx.clone(),
            config.transcriber.clone(),
        );
        if model_exists
            && let Err(err) = transcriber.start()
        {
            log::error!("Transcriber failed to start: {err}");
        }
        let agent = agent::Agent::new(controller_event_tx.clone(), config.open_api_key.clone());
        let app_state = app_state::AppState::new(config);

        let controller = controller::Controller::new(
            recorder,
            transcriber,
            controller_event_tx,
            app_event_rx,
            agent,
            app_state,
            ui_update_tx,
            controller_shutdown,
        );

        controller.run();
    });

    // Run the UI on the main thread (required by iced / macOS).
    if let Err(err) = ui.run() {
        log::error!("Failed to launch UI: {err}");
    }

    // UI has exited — signal the controller to shut down.
    log::info!("Controller shutdown requested");
    shutdown_flag.store(true, Ordering::SeqCst);
    let _ = controller_handle.join();
    log::info!("Arai shutdown complete");
}
