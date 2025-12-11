use crate::controller::Controller;
use eframe::egui::{self, Key, TextEdit, TopBottomPanel};
use once_cell::sync::OnceCell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug)]
enum UiCommand {
    AppendInput(String),
    AddMessage(String),
}

static UI_SENDER: OnceCell<Sender<UiCommand>> = OnceCell::new();

/// Public API: append text to the input box.
pub fn append_input_text(text: impl Into<String>) -> Result<(), &'static str> {
    UI_SENDER
        .get()
        .ok_or("UI not running")?
        .send(UiCommand::AppendInput(text.into()))
        .map_err(|_| "UI channel closed")
}

/// Public API: add a message bubble to the chat area.
pub fn add_chat_message(text: impl Into<String>) -> Result<(), &'static str> {
    UI_SENDER
        .get()
        .ok_or("UI not running")?
        .send(UiCommand::AddMessage(text.into()))
        .map_err(|_| "UI channel closed")
}

pub fn run_chat_ui(controller: Controller) -> eframe::Result<()> {
    let (tx, rx) = mpsc::channel();
    let _ = UI_SENDER.set(tx);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_always_on_top()
            .with_inner_size([420.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chat",
        options,
        Box::new(move |_cc| Box::new(ChatApp::new(rx, controller))),
    )
}

struct ChatApp {
    rx: Receiver<UiCommand>,
    messages: Vec<String>,
    input: String,
    controller: Controller,
    listening: bool,
    transcript_thread: Option<thread::JoinHandle<()>>,
}

impl ChatApp {
    fn new(rx: Receiver<UiCommand>, controller: Controller) -> Self {
        Self {
            rx,
            messages: Vec::new(),
            input: String::new(),
            controller,
            listening: false,
            transcript_thread: None,
        }
    }

    fn handle_commands(&mut self) {
        for cmd in self.rx.try_iter() {
            match cmd {
                UiCommand::AppendInput(text) => self.input.push_str(&text),
                UiCommand::AddMessage(text) => self.messages.push(text),
            }
        }
    }

    fn submit(&mut self) {
        if !self.input.trim().is_empty() {
            self.messages.push(self.input.trim().to_owned());
            self.input.clear();
        }
    }

    fn start_listening(&mut self) {
        if self.listening {
            return;
        }

        match self.controller.start_listening() {
            Ok(rx) => {
                let ui_sender = UI_SENDER.get().cloned();
                self.transcript_thread = Some(thread::spawn(move || {
                    if let Some(tx) = ui_sender {
                        for text in rx {
                            let _ = tx.send(UiCommand::AppendInput(format!("{text} ")));
                        }
                    }
                }));
                self.listening = true;
            }
            Err(err) => {
                self.messages.push(format!("Failed to start listening: {err:?}"));
            }
        }
    }

    fn stop_listening(&mut self) {
        let _ = self.controller.stop_listening();
        self.listening = false;
        if let Some(handle) = self.transcript_thread.take() {
            let _ = handle.join();
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_commands();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for msg in &self.messages {
                            ui.label(msg);
                            ui.add_space(4.0);
                        }
                    });
            });
        });

        TopBottomPanel::bottom("chat_input")
            .resizable(false)
            .show(ctx, |ui| {
                let mut submitted = false;
                let text_edit = TextEdit::multiline(&mut self.input)
                    .hint_text("Type a message...")
                    .desired_rows(5)
                    .desired_width(f32::INFINITY);
                let response = ui.add(text_edit);
                if response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submitted = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button_size = egui::vec2(80.0, 32.0);

                    if ui
                        .add(egui::Button::new("Send").min_size(button_size))
                        .clicked()
                    {
                        submitted = true;
                    }

                    let listen_label = if self.listening { "Listening" } else { "Listen" };
                    let listen_button = egui::SelectableLabel::new(self.listening, listen_label);
                    if ui
                        .add_sized(button_size, listen_button)
                        .clicked()
                    {
                        if self.listening {
                            self.stop_listening();
                        } else {
                            self.start_listening();
                        }
                    }
                });

                if submitted {
                    self.submit();
                    ctx.request_repaint();
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_listening();
        self.controller.shutdown();
    }
}

impl Drop for ChatApp {
    fn drop(&mut self) {
        self.stop_listening();
        self.controller.shutdown();
    }
}
