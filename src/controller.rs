use crate::agent::Agent;
use crate::channels::{AppEventReceiver, TranscribedReceiver};
use crate::messages::{AppEventKind, AppEventSource};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use crate::ui::Ui;
use log::{debug, error, info};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

pub struct Controller {
    recorder: Mutex<Option<Recorder>>,
    transcriber: Mutex<Option<Transcriber>>,
    transcript_rx: Mutex<TranscribedReceiver>,
    app_event_rx: Mutex<AppEventReceiver>,
    agent: Agent,
    ui: Ui,
    shutting_down: AtomicBool,
}

pub type ControllerHandle = Arc<Controller>;

impl Controller {
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        transcript_rx: TranscribedReceiver,
        app_event_rx: AppEventReceiver,
        agent: Agent,
        ui: Ui,
    ) -> Self {
        Self {
            recorder: Mutex::new(Some(recorder)),
            transcriber: Mutex::new(Some(transcriber)),
            transcript_rx: Mutex::new(transcript_rx),
            app_event_rx: Mutex::new(app_event_rx),
            agent,
            ui,
            shutting_down: AtomicBool::new(false),
        }
    }

    pub fn start_listening(&self) {
        if let Ok(mut recorder) = self.recorder.lock()
            && let Some(recorder) = recorder.as_mut()
        {
            info!("Controller starting recorder");
            let _ = recorder.start();
        }
    }

    pub fn stop_listening(&self) {
        if let Ok(mut recorder) = self.recorder.lock()
            && let Some(recorder) = recorder.as_mut()
        {
            info!("Controller stopping recorder");
            let _ = recorder.stop();
        }
    }

    pub fn process_text(&self, text: String) {
        debug!("Controller processing text");
        self.ui.submit_processed_text(text);
    }

    pub fn submit_text(&self, text: String) {
        debug!("Controller submitting text");
        self.agent.submit(text);
    }

    pub fn shutdown(&self) {
        info!("Controller shutdown requested");
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn run(self: Arc<Self>) {
        while !self.shutting_down.load(Ordering::SeqCst) {
            if let Ok(rx) = self.transcript_rx.lock() {
                for line in rx.try_iter() {
                    debug!("Controller received transcript");
                    self.ui
                        .append_to_text_field(format!("{text} ", text = line.text));
                }
            }

            if let Ok(app_rx) = self.app_event_rx.lock() {
                for event in app_rx.try_iter() {
                    match (event.source, event.kind) {
                        (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                            error!("Recorder event: {message}");
                            // TODO: implement recorder error handling (e.g., restart recorder or update UI)
                        }
                        (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                            error!("Transcriber event: {message}");
                        }
                        (AppEventSource::Agent, AppEventKind::Error(message)) => {
                            error!("Agent event: {message}");
                        }
                        (AppEventSource::Ui, AppEventKind::UiStartListening) => {
                            self.start_listening();
                        }
                        (AppEventSource::Ui, AppEventKind::UiStopListening) => {
                            self.stop_listening();
                        }
                        (AppEventSource::Ui, AppEventKind::UiSubmitText(text)) => {
                            self.submit_text(text);
                        }
                        (AppEventSource::Ui, AppEventKind::UiShutdown) => {
                            self.shutdown();
                        }
                        (AppEventSource::Agent, AppEventKind::AgentResponse(text)) => {
                            self.process_text(text);
                        }
                        (source, kind) => {
                            let _ = (source, kind);
                            // TODO: handle other app events
                        }
                    }
                }
            }

            self.ui.refresh();
            thread::sleep(Duration::from_millis(10));
        }

        info!("Controller shutting down");
        if let Ok(mut recorder) = self.recorder.lock()
            && let Some(mut recorder) = recorder.take()
        {
            let _ = recorder.stop();
        }
        if let Ok(mut transcriber) = self.transcriber.lock() {
            transcriber.take();
        }
    }
}
