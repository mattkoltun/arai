use crate::app_state::AppStateSnapshot;
use crate::channels::AppEventSender;
use crate::config::{AgentPrompt, TranscriberConfig};
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use iced::widget::{
    Column, button, column, container, row, scrollable, text, text_editor, text_input,
};
use iced::{Color, Element, Fill, FillPortion, Subscription, Task, Theme, time};
use log::debug;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

const MAX_PROMPTS: usize = 10;

#[derive(Clone, Debug)]
struct PromptEntry {
    name: String,
    instruction: String,
}

#[derive(Default)]
struct UiState {
    input: String,
    processed_text: Option<String>,
    processing: bool,
    listening: bool,
    needs_repaint: bool,
    config_open: bool,
    config_prompts: Vec<PromptEntry>,
    config_default: usize,
    config_model_path: String,
    config_window_seconds: String,
    config_overlap_seconds: String,
    snapshot_prompts: Vec<AgentPrompt>,
    snapshot_default: usize,
    snapshot_transcriber: Option<TranscriberConfig>,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    ToggleListen,
    Submit,
    Copy,
    EditorAction(text_editor::Action),
    OpenConfig,
    CloseConfig,
    SaveConfig,
    AddPrompt,
    RemovePrompt(usize),
    SetDefaultPrompt(usize),
    PromptNameChanged(usize, String),
    PromptInstructionChanged(usize, String),
    ModelPathChanged(String),
    WindowSecondsChanged(String),
    OverlapSecondsChanged(String),
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
            state.snapshot_prompts = snapshot.agent_prompts;
            state.snapshot_default = snapshot.default_prompt;
            state.snapshot_transcriber = Some(snapshot.transcriber);
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
                    if !ui_state.config_open {
                        state.editor = text_editor::Content::with_text(&ui_state.input);
                    }
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
        Message::OpenConfig => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                ui_state.config_prompts = ui_state
                    .snapshot_prompts
                    .iter()
                    .map(|p| PromptEntry {
                        name: p.name.clone(),
                        instruction: p.instruction.clone(),
                    })
                    .collect();
                ui_state.config_default = ui_state.snapshot_default;
                let tc = ui_state.snapshot_transcriber.clone().unwrap_or_default();
                ui_state.config_model_path = tc.model_path;
                ui_state.config_window_seconds = tc.window_seconds.to_string();
                ui_state.config_overlap_seconds = tc.overlap_seconds.to_string();
                ui_state.config_open = true;
            }
            Task::none()
        }
        Message::CloseConfig => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                ui_state.config_open = false;
            }
            Task::none()
        }
        Message::SaveConfig => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                let prompts: Vec<AgentPrompt> = ui_state
                    .config_prompts
                    .iter()
                    .filter(|p| !p.name.trim().is_empty() && !p.instruction.trim().is_empty())
                    .map(|p| AgentPrompt {
                        name: p.name.clone(),
                        instruction: p.instruction.clone(),
                    })
                    .collect();
                if prompts.is_empty() {
                    return Task::none();
                }
                let default = if ui_state.config_default < prompts.len() {
                    ui_state.config_default
                } else {
                    0
                };
                state.ui.send_event(AppEventKind::UiUpdatePrompts {
                    prompts,
                    default_prompt: default,
                });

                let window = ui_state
                    .config_window_seconds
                    .parse::<f32>()
                    .unwrap_or(2.0)
                    .max(0.1);
                let overlap = ui_state
                    .config_overlap_seconds
                    .parse::<f32>()
                    .unwrap_or(0.25)
                    .max(0.0);
                state
                    .ui
                    .send_event(AppEventKind::UiUpdateTranscriber(TranscriberConfig {
                        model_path: ui_state.config_model_path.clone(),
                        window_seconds: window,
                        overlap_seconds: overlap,
                    }));

                ui_state.config_open = false;
            }
            Task::none()
        }
        Message::AddPrompt => {
            if let Ok(mut ui_state) = state.ui.state.lock()
                && ui_state.config_prompts.len() < MAX_PROMPTS
            {
                ui_state.config_prompts.push(PromptEntry {
                    name: String::new(),
                    instruction: String::new(),
                });
            }
            Task::none()
        }
        Message::RemovePrompt(idx) => {
            if let Ok(mut ui_state) = state.ui.state.lock()
                && ui_state.config_prompts.len() > 1
                && idx < ui_state.config_prompts.len()
            {
                ui_state.config_prompts.remove(idx);
                if ui_state.config_default >= ui_state.config_prompts.len() {
                    ui_state.config_default = 0;
                } else if ui_state.config_default > idx {
                    ui_state.config_default -= 1;
                } else if ui_state.config_default == idx {
                    ui_state.config_default = 0;
                }
            }
            Task::none()
        }
        Message::SetDefaultPrompt(idx) => {
            if let Ok(mut ui_state) = state.ui.state.lock()
                && idx < ui_state.config_prompts.len()
            {
                ui_state.config_default = idx;
            }
            Task::none()
        }
        Message::PromptNameChanged(idx, value) => {
            if let Ok(mut ui_state) = state.ui.state.lock()
                && let Some(entry) = ui_state.config_prompts.get_mut(idx)
            {
                entry.name = value;
            }
            Task::none()
        }
        Message::PromptInstructionChanged(idx, value) => {
            if let Ok(mut ui_state) = state.ui.state.lock()
                && let Some(entry) = ui_state.config_prompts.get_mut(idx)
            {
                entry.instruction = value;
            }
            Task::none()
        }
        Message::ModelPathChanged(value) => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                ui_state.config_model_path = value;
            }
            Task::none()
        }
        Message::WindowSecondsChanged(value) => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                ui_state.config_window_seconds = value;
            }
            Task::none()
        }
        Message::OverlapSecondsChanged(value) => {
            if let Ok(mut ui_state) = state.ui.state.lock() {
                ui_state.config_overlap_seconds = value;
            }
            Task::none()
        }
    }
}

fn view(state: &UiRuntime) -> Element<'_, Message> {
    let ui_state = state.ui.state.lock().expect("ui state lock");

    if ui_state.config_open {
        let prompts: Vec<PromptEntry> = ui_state.config_prompts.clone();
        let default = ui_state.config_default;
        let model_path = ui_state.config_model_path.clone();
        let window_secs = ui_state.config_window_seconds.clone();
        let overlap_secs = ui_state.config_overlap_seconds.clone();
        drop(ui_state);
        return view_config(prompts, default, model_path, window_secs, overlap_secs);
    }

    let (listening, processing, has_processed, has_input, char_count) = (
        ui_state.listening,
        ui_state.processing,
        ui_state.processed_text.is_some(),
        !ui_state.input.trim().is_empty(),
        ui_state.input.chars().count(),
    );
    drop(ui_state);

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
            .style(container::rounded_box),
        container(
            button(text("\u{2699}").size(20))
                .on_press(Message::OpenConfig)
                .style(button::text)
                .padding([4, 8])
        )
        .align_right(Fill)
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

fn view_config(
    prompts: Vec<PromptEntry>,
    config_default: usize,
    model_path: String,
    window_secs: String,
    overlap_secs: String,
) -> Element<'static, Message> {
    let title = row![
        text("Settings")
            .size(22)
            .color(Color::from_rgb8(0xE5, 0xE7, 0xEB)),
        container(
            button(text("\u{2715}").size(18))
                .on_press(Message::CloseConfig)
                .style(button::text)
                .padding([4, 8])
        )
        .align_right(Fill)
    ]
    .align_y(iced::alignment::Vertical::Center);

    let prompt_count = prompts.len();
    let mut prompts_col = Column::new().spacing(12);

    for (idx, entry) in prompts.iter().enumerate() {
        let is_default = idx == config_default;

        let default_btn = button(
            text(if is_default { "\u{25C9}" } else { "\u{25CB}" })
                .size(18)
                .color(if is_default {
                    Color::from_rgb8(0x7A, 0xA2, 0xF7)
                } else {
                    Color::from_rgb8(0x9C, 0xA3, 0xAF)
                }),
        )
        .on_press(Message::SetDefaultPrompt(idx))
        .style(button::text)
        .padding([2, 4]);

        let name_input = text_input("Prompt name", &entry.name)
            .on_input(move |v| Message::PromptNameChanged(idx, v))
            .size(15)
            .padding(8);

        let instruction_input =
            text_input("Instructions (multi-line supported)", &entry.instruction)
                .on_input(move |v| Message::PromptInstructionChanged(idx, v))
                .size(14)
                .padding(8);

        let remove_btn = if prompt_count > 1 {
            button(text("\u{2212}").size(16))
                .on_press(Message::RemovePrompt(idx))
                .style(button::danger)
                .padding([4, 8])
        } else {
            button(text("\u{2212}").size(16))
                .style(button::secondary)
                .padding([4, 8])
        };

        let header_row = row![
            default_btn,
            text(if is_default { "Default" } else { "" })
                .size(12)
                .color(Color::from_rgb8(0x7A, 0xA2, 0xF7)),
            container(remove_btn).align_right(Fill),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center);

        let prompt_card = container(column![header_row, name_input, instruction_input].spacing(6))
            .padding(10)
            .style(container::rounded_box);

        prompts_col = prompts_col.push(prompt_card);
    }

    let add_btn = if prompt_count < MAX_PROMPTS {
        button(text("+ Add Prompt").size(14))
            .on_press(Message::AddPrompt)
            .style(button::secondary)
            .padding([8, 14])
    } else {
        button(text("+ Add Prompt").size(14))
            .style(button::secondary)
            .padding([8, 14])
    };

    prompts_col = prompts_col.push(add_btn);

    // Transcriber section
    let transcriber_section = container(
        column![
            text("Transcriber")
                .size(18)
                .color(Color::from_rgb8(0xE5, 0xE7, 0xEB)),
            column![
                text("Model path")
                    .size(13)
                    .color(Color::from_rgb8(0x9C, 0xA3, 0xAF)),
                text_input("models/ggml-small.en.bin", &model_path)
                    .on_input(Message::ModelPathChanged)
                    .size(14)
                    .padding(8),
            ]
            .spacing(4),
            row![
                column![
                    text("Window (seconds)")
                        .size(13)
                        .color(Color::from_rgb8(0x9C, 0xA3, 0xAF)),
                    text_input("2.0", &window_secs)
                        .on_input(Message::WindowSecondsChanged)
                        .size(14)
                        .padding(8),
                ]
                .spacing(4)
                .width(Fill),
                column![
                    text("Overlap (seconds)")
                        .size(13)
                        .color(Color::from_rgb8(0x9C, 0xA3, 0xAF)),
                    text_input("0.25", &overlap_secs)
                        .on_input(Message::OverlapSecondsChanged)
                        .size(14)
                        .padding(8),
                ]
                .spacing(4)
                .width(Fill),
            ]
            .spacing(10),
        ]
        .spacing(8),
    )
    .padding(10)
    .style(container::rounded_box);

    let scrollable_content = column![
        text("Agent Prompts")
            .size(18)
            .color(Color::from_rgb8(0xE5, 0xE7, 0xEB)),
        prompts_col,
        transcriber_section,
    ]
    .spacing(12);

    let footer = row![
        container(
            button(text("Save").size(15))
                .on_press(Message::SaveConfig)
                .style(button::primary)
                .padding([8, 20])
        )
        .align_right(Fill)
    ]
    .align_y(iced::alignment::Vertical::Center);

    let content = column![
        title,
        container(scrollable(scrollable_content).height(FillPortion(8))).padding(2),
        footer,
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
