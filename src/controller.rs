use crate::agent::Agent;
use crate::app_state::AppStateHandle;
use crate::channels::{AppEventReceiver, UiUpdateSender};
use crate::messages::{AppEventKind, AppEventSource, UiUpdate};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use log::{debug, error, info};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Lightweight handle that allows external code (e.g. main) to signal shutdown
/// without holding the entire Controller behind an Arc.
pub struct ShutdownHandle {
    flag: Arc<AtomicBool>,
}

impl ShutdownHandle {
    /// Signals the Controller's run loop to stop.
    pub fn shutdown(&self) {
        info!("Controller shutdown requested");
        self.flag.store(true, Ordering::SeqCst);
    }
}

pub struct Controller {
    recorder: Recorder,
    transcriber: Transcriber,
    app_event_rx: AppEventReceiver,
    agent: Agent,
    app_state: AppStateHandle,
    ui_update_tx: UiUpdateSender,
    shutting_down: Arc<AtomicBool>,
}

impl Controller {
    /// Creates a Controller and a [`ShutdownHandle`] that can trigger graceful
    /// shutdown from another thread.
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        app_event_rx: AppEventReceiver,
        agent: Agent,
        app_state: AppStateHandle,
        ui_update_tx: UiUpdateSender,
    ) -> (Self, ShutdownHandle) {
        let flag = Arc::new(AtomicBool::new(false));
        let handle = ShutdownHandle { flag: flag.clone() };
        let controller = Self {
            recorder,
            transcriber,
            app_event_rx,
            agent,
            app_state,
            ui_update_tx,
            shutting_down: flag,
        };
        (controller, handle)
    }

    fn start_listening(&mut self) {
        info!("Controller starting recorder");
        let _ = self.recorder.start();
    }

    fn stop_listening(&self) {
        info!("Controller signaling recorder to stop");
        self.recorder.stop_signal();
    }

    fn process_text(&self, text: String) {
        debug!("Controller processing text");
        let _ = self
            .ui_update_tx
            .send(UiUpdate::AgentResponseReceived(text));
    }

    fn submit_text(&self, text: String) {
        let instruction = self.app_state.agent_instruction();
        debug!(
            "Controller submitting text with instruction: {}",
            &instruction[..instruction.len().min(80)]
        );
        self.agent.submit(instruction, text);
    }

    /// Appends a transcription chunk to the accumulated text, adding a space
    /// separator when needed.
    fn append_transcription(accumulated: &mut String, text: &str) {
        if !accumulated.is_empty() && !accumulated.ends_with(' ') {
            accumulated.push(' ');
        }
        accumulated.push_str(text);
    }

    /// Sends a `ConfigSnapshot` to the UI so it has the current config state.
    fn send_config_snapshot(&self) {
        let snapshot = self.app_state.snapshot();
        let _ = self.ui_update_tx.send(UiUpdate::ConfigSnapshot {
            agent_prompts: snapshot.agent_prompts,
            default_prompt: snapshot.default_prompt,
            transcriber: snapshot.transcriber,
            input_devices: Recorder::list_input_devices(),
            selected_input_device: snapshot.input_device,
            global_hotkey: snapshot.global_hotkey,
        });
    }

    /// Runs the Controller event loop, consuming `self`. The loop exits when
    /// the associated [`ShutdownHandle`] signals shutdown.
    pub fn run(mut self) {
        let mut accumulated_transcription = String::new();

        // Send initial config snapshot so the UI has config before any changes.
        self.send_config_snapshot();

        while !self.shutting_down.load(Ordering::SeqCst) {
            let event = match self.app_event_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            match (event.source, event.kind) {
                (AppEventSource::Recorder, AppEventKind::Stopped) => {
                    info!("Recorder stopped, joining handle");
                    self.recorder.join_handle();
                }
                (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                    error!("Recorder event: {message}");
                    // TODO: implement recorder error handling (e.g., restart recorder or update UI)
                }
                (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                    error!("Transcriber event: {message}");
                }
                (AppEventSource::Transcriber, AppEventKind::Transcription(text)) => {
                    debug!("Controller received transcript");
                    Self::append_transcription(&mut accumulated_transcription, &text);
                    let _ = self.ui_update_tx.send(UiUpdate::TranscriptionUpdated(
                        accumulated_transcription.clone(),
                    ));
                }
                (AppEventSource::Agent, AppEventKind::Error(message)) => {
                    error!("Agent event: {message}");
                    let _ = self.ui_update_tx.send(UiUpdate::ProcessingFailed(message));
                }
                (AppEventSource::Agent, AppEventKind::AgentResponse(text)) => {
                    self.process_text(text);
                }
                (AppEventSource::Ui, AppEventKind::UiStartListening(text)) => {
                    accumulated_transcription = text;
                    self.start_listening();
                }
                (AppEventSource::Ui, AppEventKind::UiStopListening) => {
                    self.stop_listening();
                }
                (AppEventSource::Ui, AppEventKind::UiSubmitText(text)) => {
                    self.submit_text(text);
                }
                (AppEventSource::Ui, AppEventKind::UiShutdown) => {
                    self.shutting_down.store(true, Ordering::SeqCst);
                }
                (
                    AppEventSource::Ui,
                    AppEventKind::UiUpdatePrompts {
                        prompts,
                        default_prompt,
                    },
                ) => {
                    info!("Controller updating agent prompts");
                    self.app_state.update_prompts(prompts, default_prompt);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateTranscriber(transcriber_config)) => {
                    info!("Controller updating transcriber config");
                    self.app_state.update_transcriber(transcriber_config);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateInputDevice(device)) => {
                    info!("Controller updating input device: {:?}", device);
                    self.app_state.update_input_device(device.clone());
                    self.recorder.set_input_device(device);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateGlobalHotkey(hotkey)) => {
                    info!("Controller updating global hotkey: {hotkey}");
                    self.app_state.update_global_hotkey(hotkey);
                    self.send_config_snapshot();
                }
                (source, kind) => {
                    let _ = (source, kind);
                    // TODO: handle other app events
                }
            }
        }

        info!("Controller shutting down");
        drop(self.ui_update_tx);
        let mut recorder = self.recorder;
        let transcriber = self.transcriber;
        transcriber.stop();
        recorder.stop();
        drop(recorder);
        drop(transcriber);
    }
}
