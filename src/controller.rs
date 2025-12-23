use crate::channels::{AppEventReceiver, TranscribedReceiver};
use crate::messages::{AppEventKind, AppEventSource};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use crate::ui::UiHandle;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

pub struct Controller {
    recorder: Mutex<Recorder>,
    transcriber: Mutex<Option<Transcriber>>,
    transcript_rx: Mutex<TranscribedReceiver>,
    app_event_rx: Mutex<AppEventReceiver>,
    ui: UiHandle,
    shutting_down: AtomicBool,
}

pub type ControllerHandle = Arc<Controller>;

impl Controller {
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        transcript_rx: TranscribedReceiver,
        app_event_rx: AppEventReceiver,
        ui: UiHandle,
    ) -> Self {
        Self {
            recorder: Mutex::new(recorder),
            transcriber: Mutex::new(Some(transcriber)),
            transcript_rx: Mutex::new(transcript_rx),
            app_event_rx: Mutex::new(app_event_rx),
            ui,
            shutting_down: AtomicBool::new(false),
        }
    }

    pub fn start_listening(&self) {
        if let Ok(mut recorder) = self.recorder.lock() {
            let _ = recorder.start();
        }
    }

    pub fn stop_listening(&self) {
        if let Ok(mut recorder) = self.recorder.lock() {
            let _ = recorder.stop();
        }
    }

    pub fn process_text(&self, text: String) {
        self.ui.submit_processed_text(text);
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn run(self: Arc<Self>) {
        while !self.shutting_down.load(Ordering::SeqCst) {
            if let Ok(rx) = self.transcript_rx.lock() {
                for line in rx.try_iter() {
                    self.ui
                        .append_to_text_field(format!("{text} ", text = line.text));
                }
            }

            if let Ok(app_rx) = self.app_event_rx.lock() {
                for event in app_rx.try_iter() {
                    match (event.source, event.kind) {
                        (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                            eprintln!("Recorder event: {message}");
                            // TODO: implement recorder error handling (e.g., restart recorder or update UI)
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

        self.stop_listening();
        if let Ok(mut transcriber) = self.transcriber.lock() {
            transcriber.take();
        }
    }
}
