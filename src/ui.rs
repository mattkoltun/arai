use crate::app_state::AppStateSnapshot;
use crate::channels::AppEventSender;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use iced::widget::{button, column, container, row, scrollable, text, text_editor};
use iced::{Color, Element, Fill, FillPortion, Subscription, Task, Theme, time};
use log::debug;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Default)]
struct UiState {
    input: String,
    processed_text: Option<String>,
    processing: bool,
    listening: bool,
    needs_repaint: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    ToggleListen,
    Submit,
    Copy,
    EditorAction(text_editor::Action),
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

    pub fn run(&self) -> iced::Result {
        let app = self.clone();

        iced::application("Arai — Message Formatter", update, view)
            .theme(theme)
            .subscription(subscription)
            .window_size((480.0, 620.0))
            .decorations(true)
            .resizable(false)
            .run_with(move || {
                (
                    UiRuntime {
                        ui: app,
                        editor: text_editor::Content::new(),
                        status_line: "Ready".to_string(),
                    },
                    Task::none(),
                )
            })
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

    pub fn refresh_with_state(&self, snapshot: AppStateSnapshot) {
        if let Ok(mut state) = self.state.lock() {
            if !state.processing {
                state.input = snapshot.transcribed_text;
            }
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

    fn submit(&self) {
        let text = {
            let mut state = match self.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            if state.processing || state.listening || state.input.trim().is_empty() {
                return;
            }
            state.processing = true;
            state.processed_text = None;
            state.needs_repaint = true;
            self.repaint_requested.store(true, Ordering::SeqCst);
            state.input.clone()
        };

        debug!("UI submit requested");
        self.send_event(AppEventKind::UiSubmitText(text));
    }

    fn copy_processed(&self) -> Option<String> {
        let processed = {
            let state = match self.state.lock() {
                Ok(guard) => guard,
                Err(_) => return None,
            };
            if state.processing {
                return None;
            }
            state.processed_text.clone()
        };

        if processed.is_some() {
            debug!("UI copying processed text");
            self.send_event(AppEventKind::UiShutdown);
        }

        processed
    }
}

struct UiRuntime {
    ui: Ui,
    editor: text_editor::Content,
    status_line: String,
}

fn update(state: &mut UiRuntime, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            if state.ui.repaint_requested.swap(false, Ordering::SeqCst)
                && let Ok(mut ui_state) = state.ui.state.lock()
            {
                if ui_state.needs_repaint {
                    state.editor = text_editor::Content::with_text(&ui_state.input);
                    ui_state.needs_repaint = false;
                }
                state.status_line = if ui_state.processing {
                    "Submitting…".to_string()
                } else if ui_state.listening {
                    "Listening…".to_string()
                } else {
                    "Ready".to_string()
                };
            }
            Task::none()
        }
        Message::ToggleListen => {
            state.ui.toggle_listen();
            Task::none()
        }
        Message::Submit => {
            state.ui.submit();
            Task::none()
        }
        Message::Copy => {
            if let Some(text) = state.ui.copy_processed() {
                return iced::clipboard::write::<Message>(text);
            }
            Task::none()
        }
        Message::EditorAction(action) => {
            state.editor.perform(action);
            if let Ok(mut ui_state) = state.ui.state.lock()
                && !ui_state.processing
            {
                ui_state.input = state.editor.text();
            }
            Task::none()
        }
    }
}

fn view(state: &UiRuntime) -> Element<'_, Message> {
    let (listening, processing, has_processed, has_input, char_count) = {
        let ui_state = state.ui.state.lock().expect("ui state lock");
        (
            ui_state.listening,
            ui_state.processing,
            ui_state.processed_text.is_some(),
            !ui_state.input.trim().is_empty(),
            ui_state.input.chars().count(),
        )
    };

    let listen_label = if listening { "Stop" } else { "Listen" };
    let submit_label = if processing {
        "Submitting…"
    } else {
        "Submit"
    };

    let controls = row![
        button(text(listen_label).size(16))
            .on_press_maybe((!processing).then_some(Message::ToggleListen))
            .style(if listening {
                button::success
            } else {
                button::secondary
            })
            .padding([10, 18]),
        button(text(submit_label).size(16))
            .on_press_maybe((!processing && has_input && !listening).then_some(Message::Submit))
            .style(button::primary)
            .padding([10, 18]),
        button(text("Copy & close").size(16))
            .on_press_maybe((has_processed && !processing).then_some(Message::Copy))
            .style(button::secondary)
            .padding([10, 18])
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Center);

    let editor = text_editor(&state.editor)
        .placeholder("Transcribed text will appear here...")
        .on_action(Message::EditorAction)
        .padding(14)
        .size(17);

    let header = row![
        column![
            text("ARAI")
                .size(26)
                .color(Color::from_rgb8(0xE5, 0xE7, 0xEB)),
            text("Voice-to-message assistant")
                .size(14)
                .color(Color::from_rgb8(0x9C, 0xA3, 0xAF)),
        ]
        .spacing(3),
        container(text(state.status_line.as_str()).size(13))
            .padding([6, 10])
            .style(container::rounded_box)
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Center);

    let content = column![
        header,
        container(scrollable(editor).height(FillPortion(8)))
            .padding(2)
            .style(container::rounded_box),
        row![
            text(format!("{} chars", char_count))
                .size(13)
                .color(Color::from_rgb8(0x9C, 0xA3, 0xAF)),
            controls
        ]
        .spacing(12)
        .align_y(iced::alignment::Vertical::Center)
    ]
    .spacing(14)
    .padding(18)
    .height(Fill);

    container(content)
        .width(Fill)
        .height(Fill)
        .style(container::dark)
        .into()
}

fn subscription(_state: &UiRuntime) -> Subscription<Message> {
    time::every(Duration::from_millis(16)).map(|_| Message::Tick)
}

fn theme(_state: &UiRuntime) -> Theme {
    Theme::TokyoNight
}
