use crate::messages::UiCommand;
use eframe::egui::{self, Key, TextEdit, TopBottomPanel};
use once_cell::sync::OnceCell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug)]
enum InternalUi {
    AppendInput(String),
}

static UI_SENDER: OnceCell<Sender<InternalUi>> = OnceCell::new();

pub fn run_chat_ui(
    ui_cmd_tx: Sender<UiCommand>,
    transcript_rx: Receiver<String>,
) -> eframe::Result<()> {
    let (tx, rx) = mpsc::channel();
    let _ = UI_SENDER.set(tx);

    // Background thread to feed transcript text into the input box.
    thread::spawn(move || {
        if let Some(ui_tx) = UI_SENDER.get() {
            for text in transcript_rx {
                let _ = ui_tx.send(InternalUi::AppendInput(format!("{text} ")));
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_always_on_top()
            .with_inner_size([420.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chat",
        options,
        Box::new(move |_cc| {
            Box::new(ChatApp::new(ui_cmd_tx.clone(), rx))
        }),
    )
}

struct ChatApp {
    ui_cmd_tx: Sender<UiCommand>,
    internal_rx: Receiver<InternalUi>,
    messages: Vec<String>,
    input: String,
    listening: bool,
}

impl ChatApp {
    fn new(ui_cmd_tx: Sender<UiCommand>, internal_rx: Receiver<InternalUi>) -> Self {
        Self {
            ui_cmd_tx,
            internal_rx,
            messages: Vec::new(),
            input: String::new(),
            listening: false,
        }
    }

    fn handle_internal(&mut self) {
        for cmd in self.internal_rx.try_iter() {
            match cmd {
                InternalUi::AppendInput(text) => self.input.push_str(&text),
            }
        }
    }

    fn submit(&mut self) {
        if !self.input.trim().is_empty() {
            let message = self.input.trim().to_owned();
            self.messages.push(message);
            self.input.clear();
        }
    }

    fn toggle_listen(&mut self) {
        if self.listening {
            let _ = self.ui_cmd_tx.send(UiCommand::StopListening);
            self.listening = false;
        } else {
            let _ = self.ui_cmd_tx.send(UiCommand::StartListening);
            self.listening = true;
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_internal();

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
                    if ui
                        .add_sized(button_size, egui::SelectableLabel::new(self.listening, listen_label))
                        .clicked()
                    {
                        self.toggle_listen();
                    }
                });

                if submitted {
                    self.submit();
                    ctx.request_repaint();
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.ui_cmd_tx.send(UiCommand::Shutdown);
        if self.listening {
            let _ = self.ui_cmd_tx.send(UiCommand::StopListening);
        }
    }
}
