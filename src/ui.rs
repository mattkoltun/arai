use crate::channels::{AppEventSender, UiUpdateReceiver};
use crate::config::{AgentPrompt, TranscriberConfig};
use crate::global_hotkey::HotkeyHandle;
use crate::messages::{AppEvent, AppEventKind, AppEventSource, UiUpdate};
use iced::font::Family;
use iced::theme::Palette;
use iced::widget::{
    Column, button, column, container, row, scrollable, text, text_editor, text_input,
};
use iced::{
    Background, Border, Color, Element, Fill, FillPortion, Font, Subscription, Task, Theme,
    keyboard, time, window,
};
use log::debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ── Palette constants ────────────────────────────────────────────────
const BG: Color = Color::from_rgb(0.082, 0.090, 0.118); // #151724 dark graphite-blue
const SURFACE: Color = Color::from_rgb(0.118, 0.125, 0.157); // #1E2028 slightly lighter
const MUTED: Color = Color::from_rgb(0.400, 0.420, 0.490); // #66697D blue-grey
const TEXT_COLOR: Color = Color::from_rgb(0.847, 0.855, 0.894); // #D8DAE4 light
const PINK: Color = Color::from_rgb(0.976, 0.361, 0.576); // #F95C93 pastel pink
const GREEN: Color = Color::from_rgb(0.651, 0.886, 0.180); // #A6E22E
const RED: Color = Color::from_rgb(0.976, 0.149, 0.447); // #F92672

// ── Font constants ───────────────────────────────────────────────────
const ICONS: Font = Font {
    family: Family::Name("Material Icons"),
    weight: iced::font::Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

// Icon helper — uses Material Icons font, Basic shaping for PUA codepoints
fn icon(codepoint: char, size: f32) -> iced::widget::Text<'static> {
    text(codepoint.to_string())
        .font(ICONS)
        .size(size)
        .shaping(text::Shaping::Basic)
}

// ── Style: icon button — fully transparent, icon glows on hover ──────
fn icon_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered => PINK,
        button::Status::Pressed => Color::from_rgb(0.85, 0.25, 0.44),
        button::Status::Disabled => Color::from_rgb(0.25, 0.26, 0.30),
        _ => MUTED,
    };
    button::Style {
        text_color,
        background: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
    }
}

// Icon button when "active" (e.g. listening) — glows green
fn icon_btn_active(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered => Color::from_rgb(0.75, 0.95, 0.35),
        button::Status::Pressed => Color::from_rgb(0.55, 0.75, 0.12),
        _ => GREEN,
    };
    button::Style {
        text_color,
        background: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
    }
}

fn icon_btn_danger(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered => PINK,
        button::Status::Pressed => Color::from_rgb(0.85, 0.10, 0.35),
        _ => RED,
    };
    button::Style {
        text_color,
        background: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
    }
}

// ── Style: containers ────────────────────────────────────────────────
fn bg_container(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: None,
        background: Some(Background::Color(BG)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
    }
}

fn surface_container(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: None,
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 10.0.into(),
        },
        shadow: Default::default(),
    }
}

// ── Style: primary filled button (Save) ──────────────────────────────
fn primary_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(1.0, 0.43, 0.63),
        button::Status::Pressed => Color::from_rgb(0.85, 0.25, 0.44),
        _ => PINK,
    };
    button::Style {
        text_color: Color::WHITE,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 8.0.into(),
        },
        shadow: Default::default(),
    }
}

// Ghost button for config items (add prompt, etc)
fn ghost_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered => (Color::from_rgba(0.976, 0.361, 0.576, 0.12), TEXT_COLOR),
        button::Status::Pressed => (Color::from_rgba(0.976, 0.361, 0.576, 0.22), TEXT_COLOR),
        button::Status::Disabled => (Color::TRANSPARENT, Color::from_rgb(0.25, 0.26, 0.30)),
        _ => (Color::TRANSPARENT, MUTED),
    };
    button::Style {
        text_color,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 8.0.into(),
        },
        shadow: Default::default(),
    }
}

// ── Style: tab buttons ───────────────────────────────────────────────
fn tab_btn_active(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(1.0, 0.43, 0.63),
        button::Status::Pressed => Color::from_rgb(0.85, 0.25, 0.44),
        _ => PINK,
    };
    button::Style {
        text_color: Color::WHITE,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 6.0.into(),
        },
        shadow: Default::default(),
    }
}

fn tab_btn_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered => (Color::from_rgba(0.976, 0.361, 0.576, 0.10), TEXT_COLOR),
        button::Status::Pressed => (Color::from_rgba(0.976, 0.361, 0.576, 0.20), TEXT_COLOR),
        _ => (Color::TRANSPARENT, MUTED),
    };
    button::Style {
        text_color,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 6.0.into(),
        },
        shadow: Default::default(),
    }
}

// ── Style: text input / editor ───────────────────────────────────────
fn borderless_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused => PINK,
        _ => Color::TRANSPARENT,
    };
    text_input::Style {
        background: Background::Color(SURFACE),
        border: Border {
            color: border_color,
            width: if matches!(status, text_input::Status::Focused) {
                1.0
            } else {
                0.0
            },
            radius: 8.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT_COLOR,
        selection: Color::from_rgba(0.976, 0.361, 0.576, 0.3),
    }
}

fn borderless_editor(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused => PINK,
        _ => Color::TRANSPARENT,
    };
    text_editor::Style {
        background: Background::Color(SURFACE),
        border: Border {
            color: border_color,
            width: if matches!(status, text_editor::Status::Focused) {
                1.0
            } else {
                0.0
            },
            radius: 8.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT_COLOR,
        selection: Color::from_rgba(0.976, 0.361, 0.576, 0.3),
    }
}

// ── Config tab enum ──────────────────────────────────────────────────
#[derive(Clone, Debug, Default, PartialEq)]
enum ConfigTab {
    #[default]
    Setup,
    Instructions,
    Advanced,
}

// ── App mode state machine ──────────────────────────────────────────
#[derive(Clone, Debug, Default, PartialEq)]
enum AppMode {
    #[default]
    Idle,
    Listening,
    Processing,
}

const MAX_PROMPTS: usize = 10;

struct TranscriberFields {
    model_path: String,
    window_secs: String,
    overlap_secs: String,
    silence_thresh: String,
}

#[derive(Clone, Debug, Default)]
struct PromptEntry {
    name: String,
    instruction: String,
}

#[derive(Clone)]
pub struct Ui {
    app_event_tx: AppEventSender,
    hotkey_handle: Option<Arc<HotkeyHandle>>,
    ui_update_rx: Arc<Mutex<Option<UiUpdateReceiver>>>,
}

impl Ui {
    pub fn new(
        app_event_tx: AppEventSender,
        hotkey_handle: Option<HotkeyHandle>,
        ui_update_rx: UiUpdateReceiver,
    ) -> Self {
        Self {
            app_event_tx,
            hotkey_handle: hotkey_handle.map(Arc::new),
            ui_update_rx: Arc::new(Mutex::new(Some(ui_update_rx))),
        }
    }

    pub fn run(self) -> iced::Result {
        iced::application("Arai", update, view)
            .theme(theme)
            .subscription(subscription)
            .window_size((480.0, 620.0))
            .decorations(false)
            .resizable(false)
            .font(include_bytes!("../assets/fonts/MaterialIcons-Regular.ttf").as_slice())
            .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
            .run_with(move || {
                (
                    UiRuntime {
                        app_event_tx: self.app_event_tx,
                        hotkey_handle: self.hotkey_handle,
                        ui_update_rx: self.ui_update_rx,
                        editor: text_editor::Content::new(),
                        status_line: "Ready".to_string(),
                        instruction_editors: Vec::new(),
                        window_id: None,
                        pulse_phase: 0.0,
                        input: String::new(),
                        processed_text: None,
                        mode: AppMode::Idle,
                        config_open: false,
                        config_prompts: Vec::new(),
                        config_default: 0,
                        config_model_path: String::new(),
                        config_window_seconds: String::new(),
                        config_overlap_seconds: String::new(),
                        config_silence_threshold: String::new(),
                        config_tab: ConfigTab::default(),
                        snapshot_prompts: Vec::new(),
                        snapshot_default: 0,
                        snapshot_transcriber: None,
                    },
                    Task::none(),
                )
            })
    }
}

struct UiRuntime {
    app_event_tx: AppEventSender,
    hotkey_handle: Option<Arc<HotkeyHandle>>,
    ui_update_rx: Arc<Mutex<Option<UiUpdateReceiver>>>,
    editor: text_editor::Content,
    status_line: String,
    instruction_editors: Vec<text_editor::Content>,
    window_id: Option<window::Id>,
    /// Pulse phase in radians for the processing indicator animation.
    pulse_phase: f32,
    // State previously in UiState:
    input: String,
    processed_text: Option<String>,
    mode: AppMode,
    config_open: bool,
    config_prompts: Vec<PromptEntry>,
    config_default: usize,
    config_model_path: String,
    config_window_seconds: String,
    config_overlap_seconds: String,
    config_silence_threshold: String,
    config_tab: ConfigTab,
    snapshot_prompts: Vec<AgentPrompt>,
    snapshot_default: usize,
    snapshot_transcriber: Option<TranscriberConfig>,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    UiUpdateReceived(UiUpdate),
    EditorAction(text_editor::Action),
    ToggleListen,
    Submit,
    Copy,
    OpenConfig,
    CloseConfig,
    SaveConfig,
    AddPrompt,
    RemovePrompt(usize),
    SetDefaultPrompt(usize),
    PromptNameChanged(usize, String),
    PromptInstructionAction(usize, text_editor::Action),
    ModelPathChanged(String),
    WindowSecondsChanged(String),
    OverlapSecondsChanged(String),
    SilenceThresholdChanged(String),
    SwitchConfigTab(ConfigTab),
    Shutdown,
    KeyPressed(keyboard::Key, keyboard::Modifiers),
    WindowOpened(window::Id),
}

impl UiRuntime {
    fn send_event(&self, kind: AppEventKind) {
        let _ = self.app_event_tx.send(AppEvent {
            source: AppEventSource::Ui,
            kind,
        });
    }

    fn toggle_listen(&mut self) {
        if self.mode == AppMode::Processing {
            return;
        }
        if self.mode == AppMode::Listening {
            debug!("UI stopping listen");
            self.send_event(AppEventKind::UiStopListening);
            self.mode = AppMode::Idle;
            self.status_line = "Ready".to_string();
        } else {
            debug!("UI starting listen");
            self.send_event(AppEventKind::UiStartListening(self.input.clone()));
            self.mode = AppMode::Listening;
            self.status_line = "Listening...".to_string();
        }
    }

    fn submit(&mut self) {
        if self.mode != AppMode::Idle || self.input.trim().is_empty() {
            return;
        }
        self.mode = AppMode::Processing;
        self.processed_text = None;
        self.status_line = "Processing...".to_string();
        debug!("UI submit requested");
        self.send_event(AppEventKind::UiSubmitText(self.input.clone()));
    }
}

fn update(state: &mut UiRuntime, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            // Poll global hotkey — toggle listen and focus window on press.
            let hotkey_fired = if let Some(ref handle) = state.hotkey_handle
                && handle.poll_event()
            {
                state.toggle_listen();
                true
            } else {
                false
            };

            // Advance pulse animation while processing (~2.4 Hz cycle at 16ms ticks).
            if state.mode == AppMode::Processing {
                state.pulse_phase += 0.15;
            } else {
                state.pulse_phase = 0.0;
            }

            if hotkey_fired {
                if let Some(id) = state.window_id {
                    window::gain_focus(id)
                } else {
                    Task::none()
                }
            } else {
                Task::none()
            }
        }
        Message::UiUpdateReceived(update) => {
            match update {
                UiUpdate::TranscriptionUpdated(text) => {
                    if state.mode == AppMode::Listening && state.input != text {
                        state.input = text;
                        state.editor = text_editor::Content::with_text(&state.input);
                        state.status_line = "Listening...".to_string();
                    }
                }
                UiUpdate::AgentResponseReceived(text) => {
                    state.processed_text = Some(text.clone());
                    state.input = text;
                    state.mode = AppMode::Idle;
                    state.editor = text_editor::Content::with_text(&state.input);
                    state.status_line = "Ready".to_string();
                }
                UiUpdate::ProcessingFailed(message) => {
                    log::error!("Processing failed: {message}");
                    state.mode = AppMode::Idle;
                    state.status_line = "Error — try again".to_string();
                }
                UiUpdate::ConfigSnapshot {
                    agent_prompts,
                    default_prompt,
                    transcriber,
                } => {
                    state.snapshot_prompts = agent_prompts;
                    state.snapshot_default = default_prompt;
                    state.snapshot_transcriber = Some(transcriber);
                }
            }
            Task::none()
        }
        Message::EditorAction(action) => {
            state.editor.perform(action);
            if state.mode == AppMode::Idle {
                state.input = state.editor.text();
            }
            Task::none()
        }
        Message::ToggleListen => {
            state.toggle_listen();
            Task::none()
        }
        Message::Submit => {
            state.submit();
            Task::none()
        }
        Message::Copy => {
            if state.mode != AppMode::Idle || state.input.trim().is_empty() {
                return Task::none();
            }
            debug!("UI copying text to clipboard");
            let minimize = state
                .window_id
                .map(|id| window::minimize(id, true))
                .unwrap_or(Task::none());
            Task::batch([
                iced::clipboard::write::<Message>(state.input.clone()),
                minimize,
            ])
        }
        Message::OpenConfig => {
            state.config_prompts = state
                .snapshot_prompts
                .iter()
                .map(|p| PromptEntry {
                    name: p.name.clone(),
                    instruction: p.instruction.clone(),
                })
                .collect();
            state.config_default = state.snapshot_default;
            let tc = state.snapshot_transcriber.clone().unwrap_or_default();
            state.config_model_path = tc.model_path;
            state.config_window_seconds = tc.window_seconds.to_string();
            state.config_overlap_seconds = tc.overlap_seconds.to_string();
            state.config_silence_threshold = tc.silence_threshold.to_string();
            state.config_tab = ConfigTab::Setup;
            state.config_open = true;

            state.instruction_editors = state
                .config_prompts
                .iter()
                .map(|p| text_editor::Content::with_text(&p.instruction))
                .collect();
            Task::none()
        }
        Message::CloseConfig => {
            state.config_open = false;
            Task::none()
        }
        Message::SaveConfig => {
            for (i, editor) in state.instruction_editors.iter().enumerate() {
                if i < state.config_prompts.len() {
                    state.config_prompts[i].instruction = editor.text();
                }
            }

            let prompts: Vec<AgentPrompt> = state
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
            let default = if state.config_default < prompts.len() {
                state.config_default
            } else {
                0
            };
            state.send_event(AppEventKind::UiUpdatePrompts {
                prompts,
                default_prompt: default,
            });

            let window = state
                .config_window_seconds
                .parse::<f32>()
                .unwrap_or(2.0)
                .max(0.1);
            let overlap = state
                .config_overlap_seconds
                .parse::<f32>()
                .unwrap_or(0.25)
                .max(0.0);
            let silence = state
                .config_silence_threshold
                .parse::<f32>()
                .unwrap_or(0.005)
                .max(0.0);
            state.send_event(AppEventKind::UiUpdateTranscriber(TranscriberConfig {
                model_path: state.config_model_path.clone(),
                window_seconds: window,
                overlap_seconds: overlap,
                silence_threshold: silence,
            }));

            state.config_open = false;
            Task::none()
        }
        Message::AddPrompt => {
            if state.config_prompts.len() < MAX_PROMPTS {
                let next_num = state.config_prompts.len() + 1;
                state.config_prompts.push(PromptEntry {
                    name: format!("Prompt {}", next_num),
                    instruction: String::new(),
                });
                state.instruction_editors.push(text_editor::Content::new());
            }
            Task::none()
        }
        Message::RemovePrompt(idx) => {
            if state.config_prompts.len() > 1 && idx < state.config_prompts.len() {
                state.config_prompts.remove(idx);
                state.instruction_editors.remove(idx);

                if state.config_default >= state.config_prompts.len() {
                    state.config_default = 0;
                } else if state.config_default > idx {
                    state.config_default -= 1;
                } else if state.config_default == idx {
                    state.config_default = 0;
                }
            }
            Task::none()
        }
        Message::SetDefaultPrompt(idx) => {
            if idx < state.config_prompts.len() {
                state.config_default = idx;
            }
            Task::none()
        }
        Message::PromptNameChanged(idx, value) => {
            if let Some(entry) = state.config_prompts.get_mut(idx) {
                entry.name = value;
            }
            Task::none()
        }
        Message::PromptInstructionAction(idx, action) => {
            if idx < state.instruction_editors.len() {
                state.instruction_editors[idx].perform(action);

                if idx < state.config_prompts.len() {
                    state.config_prompts[idx].instruction = state.instruction_editors[idx].text();
                }
            }
            Task::none()
        }
        Message::ModelPathChanged(value) => {
            state.config_model_path = value;
            Task::none()
        }
        Message::WindowSecondsChanged(value) => {
            state.config_window_seconds = value;
            Task::none()
        }
        Message::OverlapSecondsChanged(value) => {
            state.config_overlap_seconds = value;
            Task::none()
        }
        Message::SilenceThresholdChanged(value) => {
            state.config_silence_threshold = value;
            Task::none()
        }
        Message::Shutdown => {
            state.send_event(AppEventKind::UiShutdown);
            iced::exit()
        }
        Message::SwitchConfigTab(tab) => {
            state.config_tab = tab;
            Task::none()
        }
        Message::WindowOpened(id) => {
            state.window_id = Some(id);
            Task::none()
        }
        Message::KeyPressed(key, modifiers) => match key {
            keyboard::Key::Named(keyboard::key::Named::Enter) if modifiers.command() => {
                state.submit();
                Task::none()
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                if state.config_open {
                    state.config_open = false;
                }
                Task::none()
            }
            _ => Task::none(),
        },
    }
}

// ── Views ────────────────────────────────────────────────────────────

fn view(state: &UiRuntime) -> Element<'_, Message> {
    if state.config_open {
        let transcriber_fields = TranscriberFields {
            model_path: state.config_model_path.clone(),
            window_secs: state.config_window_seconds.clone(),
            overlap_secs: state.config_overlap_seconds.clone(),
            silence_thresh: state.config_silence_threshold.clone(),
        };
        return view_config(
            state,
            state.config_prompts.clone(),
            state.config_default,
            transcriber_fields,
            state.config_tab.clone(),
        );
    }

    let listening = state.mode == AppMode::Listening;
    let processing = state.mode == AppMode::Processing;

    view_main(
        state,
        listening,
        processing,
        !state.input.trim().is_empty(),
        state.input.chars().count(),
    )
}

fn view_main<'a>(
    state: &'a UiRuntime,
    listening: bool,
    processing: bool,
    has_text: bool,
    char_count: usize,
) -> Element<'a, Message> {
    // close: E5CD
    let close_btn = button(icon('\u{E5CD}', 20.0))
        .style(icon_btn)
        .padding(6)
        .on_press(Message::Shutdown);

    let top_bar =
        container(row![container(close_btn).align_right(Fill)].align_y(iced::Alignment::Center))
            .padding([10, 14])
            .width(Fill);

    let mut editor_area = text_editor(&state.editor)
        .style(borderless_editor)
        .padding(16)
        .height(Fill);
    if !listening && !processing {
        editor_area = editor_area.on_action(Message::EditorAction);
    }

    let char_count_text = text(format!("{} chars", char_count)).size(12).color(MUTED);

    // mic: E029=mic, E02B=mic_off
    let mic_btn = if listening {
        button(icon('\u{E02B}', 22.0))
            .style(icon_btn_active)
            .padding([8, 12])
            .on_press_maybe((!processing).then_some(Message::ToggleListen))
    } else {
        button(icon('\u{E029}', 22.0))
            .style(icon_btn)
            .padding([8, 12])
            .on_press_maybe((!processing).then_some(Message::ToggleListen))
    };

    // send: E163
    let send_btn = if processing {
        // Pulsate the send icon red while processing.
        let t = state.pulse_phase.sin() * 0.5 + 0.5; // 0.0 – 1.0
        let pulse_color = Color::from_rgb(
            0.976 * t + 0.25 * (1.0 - t), // red channel: bright red ↔ dim
            0.149 * t + 0.10 * (1.0 - t), // green channel
            0.447 * t + 0.15 * (1.0 - t), // blue channel
        );
        button(icon('\u{E163}', 22.0).color(pulse_color))
            .style(icon_btn)
            .padding([8, 12])
    } else {
        button(icon('\u{E163}', 22.0))
            .style(icon_btn)
            .padding([8, 12])
            .on_press_maybe((!listening).then_some(Message::Submit))
    };

    // copy: E14D
    let copy_btn = button(icon('\u{E14D}', 22.0))
        .style(icon_btn)
        .padding([8, 12])
        .on_press_maybe((has_text && !processing && !listening).then_some(Message::Copy));

    // settings: E8B8
    let settings_btn = button(icon('\u{E8B8}', 22.0))
        .style(icon_btn)
        .padding([8, 12])
        .on_press_maybe((!listening && !processing).then_some(Message::OpenConfig));

    let button_group = row![mic_btn, send_btn, copy_btn, settings_btn]
        .spacing(16)
        .align_y(iced::Alignment::Center);

    let bottom_bar = column![
        container(button_group).center_x(Fill),
        container(char_count_text).padding([4, 18])
    ]
    .spacing(6);

    let body = column![
        container(editor_area)
            .style(surface_container)
            .padding(4)
            .height(FillPortion(8)),
        container(bottom_bar).height(FillPortion(2))
    ]
    .spacing(8)
    .padding([0, 14]);

    let content = column![top_bar, body];

    container(content)
        .style(bg_container)
        .width(Fill)
        .height(Fill)
        .into()
}

fn view_config<'a>(
    state: &'a UiRuntime,
    prompts: Vec<PromptEntry>,
    config_default: usize,
    tf: TranscriberFields,
    config_tab: ConfigTab,
) -> Element<'a, Message> {
    let setup_btn = button(text("Setup").size(13))
        .style(if config_tab == ConfigTab::Setup {
            tab_btn_active
        } else {
            tab_btn_inactive
        })
        .padding([6, 14])
        .on_press(Message::SwitchConfigTab(ConfigTab::Setup));

    let instructions_btn = button(text("Instructions").size(13))
        .style(if config_tab == ConfigTab::Instructions {
            tab_btn_active
        } else {
            tab_btn_inactive
        })
        .padding([6, 14])
        .on_press(Message::SwitchConfigTab(ConfigTab::Instructions));

    let advanced_btn = button(text("Advanced").size(13))
        .style(if config_tab == ConfigTab::Advanced {
            tab_btn_active
        } else {
            tab_btn_inactive
        })
        .padding([6, 14])
        .on_press(Message::SwitchConfigTab(ConfigTab::Advanced));

    // close: E5CD
    let close_btn = button(icon('\u{E5CD}', 20.0))
        .style(icon_btn)
        .padding(6)
        .on_press(Message::CloseConfig);

    let top_bar = container(
        row![
            row![setup_btn, instructions_btn, advanced_btn]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            container(close_btn).align_right(Fill)
        ]
        .align_y(iced::Alignment::Center),
    )
    .padding([10, 14])
    .width(Fill);

    let tab_content = match config_tab {
        ConfigTab::Setup => view_setup_tab(
            &tf.model_path,
            &tf.window_secs,
            &tf.overlap_secs,
            &tf.silence_thresh,
        ),
        ConfigTab::Instructions => view_instructions_tab(state, &prompts, config_default),
        ConfigTab::Advanced => view_advanced_tab(),
    };

    let save_btn = button(text("Save").size(13))
        .style(primary_btn)
        .padding([8, 20])
        .on_press(Message::SaveConfig);

    let bottom_bar = container(save_btn)
        .width(Fill)
        .padding([10, 14])
        .align_x(iced::alignment::Horizontal::Right);

    let content = column![
        top_bar,
        container(scrollable(tab_content).height(Fill)).height(FillPortion(9)),
        container(bottom_bar).height(FillPortion(1))
    ];

    container(content)
        .style(bg_container)
        .width(Fill)
        .height(Fill)
        .into()
}

fn view_setup_tab(
    model_path: &str,
    window_secs: &str,
    overlap_secs: &str,
    silence_thresh: &str,
) -> Column<'static, Message> {
    let model_path_input = text_input("Model path", model_path)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::ModelPathChanged);

    let window_secs_input = text_input("Window seconds", window_secs)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::WindowSecondsChanged);

    let overlap_secs_input = text_input("Overlap seconds", overlap_secs)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::OverlapSecondsChanged);

    let silence_thresh_input = text_input("Silence threshold", silence_thresh)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::SilenceThresholdChanged);

    let card_content = column![
        text("Transcriber").size(15).color(TEXT_COLOR),
        column![text("Model Path").size(11).color(MUTED), model_path_input].spacing(4),
        column![text("Window (s)").size(11).color(MUTED), window_secs_input].spacing(4),
        column![
            text("Overlap (s)").size(11).color(MUTED),
            overlap_secs_input
        ]
        .spacing(4),
        column![
            text("Silence Threshold").size(11).color(MUTED),
            silence_thresh_input
        ]
        .spacing(4),
    ]
    .spacing(10)
    .padding(14);

    column![container(card_content).style(surface_container).width(Fill)]
        .spacing(12)
        .padding(14)
}

fn view_instructions_tab<'a>(
    state: &'a UiRuntime,
    prompts: &[PromptEntry],
    config_default: usize,
) -> Column<'a, Message> {
    let mut prompts_column = column![].spacing(10);

    for (idx, prompt) in prompts.iter().enumerate() {
        let is_default = config_default == idx;

        // radio: E837=radio_checked, E836=radio_unchecked
        let radio_btn = button(if is_default {
            icon('\u{E837}', 20.0)
        } else {
            icon('\u{E836}', 20.0)
        })
        .style(if is_default {
            icon_btn_active
        } else {
            icon_btn
        })
        .padding(4)
        .on_press(Message::SetDefaultPrompt(idx));

        let name_input = text_input("Name", &prompt.name)
            .style(borderless_input)
            .padding(10)
            .on_input(move |val| Message::PromptNameChanged(idx, val));

        let instruction_editor = text_editor(&state.instruction_editors[idx])
            .style(borderless_editor)
            .padding(10)
            .height(100)
            .on_action(move |action| Message::PromptInstructionAction(idx, action));

        // delete: E872
        let remove_btn = if prompts.len() > 1 {
            button(icon('\u{E872}', 18.0))
                .style(icon_btn_danger)
                .padding(4)
                .on_press(Message::RemovePrompt(idx))
        } else {
            button(icon('\u{E872}', 18.0)).style(icon_btn).padding(4)
        };

        let prompt_card = column![
            row![radio_btn, container(name_input).width(Fill), remove_btn]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            column![
                text("Instruction").size(11).color(MUTED),
                instruction_editor
            ]
            .spacing(4)
        ]
        .spacing(8)
        .padding(12);

        prompts_column =
            prompts_column.push(container(prompt_card).style(surface_container).width(Fill));
    }

    // add: E145
    if prompts.len() < MAX_PROMPTS {
        let add_btn = button(
            row![icon('\u{E145}', 18.0), text("Add Prompt").size(13)]
                .spacing(6)
                .align_y(iced::Alignment::Center),
        )
        .style(ghost_btn)
        .padding([6, 14])
        .on_press(Message::AddPrompt);
        prompts_column = prompts_column.push(add_btn);
    }

    column![prompts_column].spacing(12).padding(14)
}

fn view_advanced_tab() -> Column<'static, Message> {
    column![text("More settings coming soon.").size(13).color(MUTED)]
        .spacing(12)
        .padding(14)
}

fn subscription(state: &UiRuntime) -> Subscription<Message> {
    let ui_update_rx = state.ui_update_rx.clone();
    Subscription::batch([
        time::every(Duration::from_millis(16)).map(|_| Message::Tick),
        keyboard::on_key_press(|key, modifiers| Some(Message::KeyPressed(key, modifiers))),
        window::open_events().map(Message::WindowOpened),
        Subscription::run_with_id(
            "ui-updates",
            iced::stream::channel(100, move |mut sender| async move {
                let rx = {
                    let mut guard = ui_update_rx.lock().unwrap();
                    guard.take()
                };
                if let Some(rx) = rx {
                    std::thread::spawn(move || {
                        while let Ok(update) = rx.recv() {
                            if sender.try_send(Message::UiUpdateReceived(update)).is_err() {
                                break;
                            }
                        }
                    });
                }
                std::future::pending::<()>().await;
            }),
        ),
    ])
}

fn theme(_state: &UiRuntime) -> Theme {
    Theme::custom(
        "Arai".to_string(),
        Palette {
            background: BG,
            text: TEXT_COLOR,
            primary: PINK,
            success: GREEN,
            danger: RED,
        },
    )
}
