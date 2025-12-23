use crate::channels::AppEventSender;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use eframe::egui::{self, TextEdit, TopBottomPanel};
use log::{debug, info};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct UiState {
    input: String,
    processed_text: Option<String>,
    processing: bool,
    listening: bool,
    needs_repaint: bool,
}

#[derive(Clone)]
pub struct Ui {
    state: Arc<Mutex<UiState>>,
    repaint_requested: Arc<AtomicBool>,
    app_event_tx: AppEventSender,
}

impl Ui {
    pub fn new(app_event_tx: AppEventSender) -> Self {
        let state = Arc::new(Mutex::new(UiState::default()));
        let repaint_requested = Arc::new(AtomicBool::new(false));
        Self {
            state,
            repaint_requested,
            app_event_tx,
        }
    }

    pub fn run(&self) -> eframe::Result<()> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_always_on_top()
                .with_inner_size([420.0, 560.0]),
            ..Default::default()
        };

        let app = self.clone();

        eframe::run_native(
            "Message Formatter",
            options,
            Box::new(move |_cc| {
                info!("UI initialized");
                Box::new(app.clone())
            }),
        )
    }

    pub fn append_to_text_field(&self, text: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            let text = text.into();
            if !state.input.ends_with(' ') && !state.input.is_empty() {
                state.input.push(' ');
            }
            state.input.push_str(&text);
            state.needs_repaint = true;
            self.repaint_requested.store(true, Ordering::SeqCst);
        }
    }

    pub fn submit_processed_text(&self, text: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            let text = text.into();
            state.processed_text = Some(text.clone());
            state.input = text;
            state.processing = false;
            state.needs_repaint = true;
            self.repaint_requested.store(true, Ordering::SeqCst);
        }
    }

    pub fn refresh(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.needs_repaint = true;
            self.repaint_requested.store(true, Ordering::SeqCst);
        }
    }

    fn send_event(&self, kind: AppEventKind) {
        let _ = self.app_event_tx.send(AppEvent {
            source: AppEventSource::Ui,
            kind,
        });
    }

    fn toggle_listen(&self) {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if state.processing {
            return;
        }

        if state.listening {
            debug!("UI stopping listen");
            self.send_event(AppEventKind::UiStopListening);
            state.listening = false;
        } else {
            debug!("UI starting listen");
            self.send_event(AppEventKind::UiStartListening);
            state.listening = true;
        }
        state.needs_repaint = true;
        self.repaint_requested.store(true, Ordering::SeqCst);
    }

    fn process(&self) {
        let (should_send, text) = {
            let mut state = match self.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            if state.processing || state.input.trim().is_empty() {
                return;
            }
            state.processing = true;
            state.processed_text = None;
            state.needs_repaint = true;
            self.repaint_requested.store(true, Ordering::SeqCst);
            (true, state.input.clone())
        };

        if should_send {
            debug!("UI processing requested");
            self.send_event(AppEventKind::UiProcessText(text));
        }
    }

    fn copy_processed(&self, ctx: &egui::Context) {
        let processed = {
            let state = match self.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            if state.processing {
                return;
            }
            state.processed_text.clone()
        };

        if let Some(text) = processed {
            debug!("UI copying processed text");
            ctx.output_mut(|o| o.copied_text = text.clone());
            self.send_event(AppEventKind::UiShutdown);
        }
    }
}

impl eframe::App for Ui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.repaint_requested.swap(false, Ordering::SeqCst) {
            ctx.request_repaint();
        }

        TopBottomPanel::bottom("controls")
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_height(72.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let (listening, processing, has_processed, has_input) = {
                        let state = self.state.lock().unwrap();
                        (
                            state.listening,
                            state.processing,
                            state.processed_text.is_some(),
                            !state.input.trim().is_empty(),
                        )
                    };

                    let button_size = egui::vec2(120.0, 36.0);

                    let listen_label = if listening {
                        "Stop Listening"
                    } else {
                        "Listen"
                    };
                    let listen_response = ui
                        .add_enabled_ui(!processing, |ui| {
                            ui.add_sized(
                                button_size,
                                egui::SelectableLabel::new(listening, listen_label),
                            )
                        })
                        .inner;
                    if listen_response.clicked() {
                        self.toggle_listen();
                    }

                    ui.add_space(8.0);

                    let process_label = if processing {
                        "Processing..."
                    } else {
                        "Process"
                    };
                    let process_response = ui.add_enabled(
                        !processing && has_input,
                        egui::Button::new(process_label).min_size(button_size),
                    );
                    if process_response.clicked() {
                        self.process();
                    }
                    if processing {
                        let mut overlay = ui.child_ui(
                            process_response.rect,
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        );
                        overlay.add(egui::Spinner::new());
                    }

                    ui.add_space(8.0);

                    let copy_enabled = has_processed && !processing;
                    if ui
                        .add_enabled(
                            copy_enabled,
                            egui::Button::new("Copy").min_size(button_size),
                        )
                        .clicked()
                    {
                        self.copy_processed(ctx);
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut state = self.state.lock().unwrap();
            let available = ui.available_size();
            ui.add_enabled_ui(!state.processing, |ui| {
                ui.add_sized(
                    available,
                    TextEdit::multiline(&mut state.input)
                        .desired_width(f32::INFINITY)
                        .desired_rows(16)
                        .hint_text("Transcribed text will appear here..."),
                );
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        info!("UI exit requested");
        self.send_event(AppEventKind::UiShutdown);
    }
}
