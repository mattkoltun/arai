use crate::app_state::AppState;
use crate::channels::{
    AppChannels, AppEventReceiver, AppEventSender, AudioChannels, UiUpdateSender,
};
use crate::config::{Config, ConfigError};
use crate::controller::Controller;
use crate::global_hotkey::HotkeyHandle;
use crate::llm::LlmWorker;
use crate::logger::{self, LogConfig};
use crate::openai_connector::OpenAiConnector;
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use crate::ui::Ui;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

/// Builds and runs the full Arai application runtime.
pub(crate) struct App {
    ui: Ui,
    shutdown_flag: Arc<AtomicBool>,
    controller_handle: JoinHandle<()>,
}

struct LoadedConfig {
    config: Config,
    model_exists: bool,
    api_key_exists: bool,
}

struct ControllerRuntime {
    config: Config,
    model_exists: bool,
    app_event_tx: AppEventSender,
    app_event_rx: AppEventReceiver,
    ui_update_tx: UiUpdateSender,
    shutdown_flag: Arc<AtomicBool>,
}

/// Startup failures that prevent the app runtime from being created.
#[derive(Debug)]
pub(crate) enum AppBuildError {
    Config(ConfigError),
    LlmConnector(crate::llm::LlmError),
}

impl Display for AppBuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(err) => write!(f, "Config error: {err}"),
            Self::LlmConnector(err) => write!(f, "LLM connector error: {err}"),
        }
    }
}

impl std::error::Error for AppBuildError {}

impl From<ConfigError> for AppBuildError {
    fn from(err: ConfigError) -> Self {
        Self::Config(err)
    }
}

impl From<crate::llm::LlmError> for AppBuildError {
    fn from(err: crate::llm::LlmError) -> Self {
        Self::LlmConnector(err)
    }
}

/// Runtime failures that occur after the app has been constructed.
#[derive(Debug)]
pub(crate) enum AppRunError {
    Ui(iced::Error),
    ControllerThreadPanicked,
}

impl Display for AppRunError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ui(err) => write!(f, "Failed to launch UI: {err}"),
            Self::ControllerThreadPanicked => write!(f, "Controller thread panicked"),
        }
    }
}

impl std::error::Error for AppRunError {}

impl App {
    /// Creates the full application runtime, including channels, UI, and controller thread.
    pub(crate) fn build() -> Result<Self, AppBuildError> {
        let loaded = LoadedConfig::load()?;
        loaded.init_logger();

        let AppChannels {
            app_event_tx,
            app_event_rx,
            ui_update_tx,
            ui_update_rx,
        } = AppChannels::new();

        let hotkey_handle = HotkeyHandle::register(&loaded.config.global_hotkey);
        let ui = Ui::new(
            app_event_tx.clone(),
            hotkey_handle,
            ui_update_rx,
            loaded.model_exists,
            loaded.api_key_exists,
        );

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let controller_handle = Self::spawn_controller(ControllerRuntime {
            config: loaded.config,
            model_exists: loaded.model_exists,
            app_event_tx,
            app_event_rx,
            ui_update_tx,
            shutdown_flag: Arc::clone(&shutdown_flag),
        });

        Ok(Self {
            ui,
            shutdown_flag,
            controller_handle,
        })
    }

    /// Runs the UI on the main thread and coordinates clean shutdown.
    pub(crate) fn run(self) -> Result<(), AppRunError> {
        let ui_result = self.ui.run().map_err(AppRunError::Ui);

        log::info!("Controller shutdown requested");
        self.shutdown_flag.store(true, Ordering::SeqCst);

        let join_result = self
            .controller_handle
            .join()
            .map_err(|_| AppRunError::ControllerThreadPanicked);

        log::info!("Arai shutdown complete");

        ui_result?;
        join_result?;
        Ok(())
    }

    fn spawn_controller(runtime: ControllerRuntime) -> JoinHandle<()> {
        thread::spawn(move || match build_controller(runtime) {
            Ok(controller) => controller.run(),
            Err(err) => log::error!("Failed to build controller runtime: {err}"),
        })
    }
}

impl LoadedConfig {
    fn load() -> Result<Self, ConfigError> {
        let config = Config::load()?;
        let model_exists = Path::new(&config.transcriber.model_path).exists();
        let api_key_exists = !config.open_api_key.is_empty();
        Ok(Self {
            config,
            model_exists,
            api_key_exists,
        })
    }

    fn init_logger(&self) {
        if let Err(err) = logger::init_with_config(LogConfig {
            level: self.config.parsed_log_level(),
            path: self.config.parsed_log_path(),
        }) {
            eprintln!("Failed to init logger: {err}");
        }
        log::info!("Starting Arai");
    }
}

fn build_controller(runtime: ControllerRuntime) -> Result<Controller, AppBuildError> {
    let ControllerRuntime {
        config,
        model_exists,
        app_event_tx,
        app_event_rx,
        ui_update_tx,
        shutdown_flag,
    } = runtime;

    let AudioChannels { audio_tx, audio_rx } = AudioChannels::new();
    let recorder = Recorder::new(audio_tx, app_event_tx.clone(), config.input_device.clone());
    let mut transcriber =
        Transcriber::new(audio_rx, app_event_tx.clone(), config.transcriber.clone());
    if model_exists && let Err(err) = transcriber.start() {
        log::error!("Transcriber failed to start: {err}");
    }
    let connector = OpenAiConnector::new(config.open_api_key.clone())?;
    let llm_worker = LlmWorker::new(app_event_tx.clone(), Box::new(connector));
    let app_state = AppState::new(config);

    Ok(Controller::new(
        recorder,
        transcriber,
        app_event_tx,
        app_event_rx,
        llm_worker,
        app_state,
        ui_update_tx,
        shutdown_flag,
    ))
}
