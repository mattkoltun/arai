use crate::channels::{AppEventSender, UiUpdateReceiver};
use crate::config::{AgentPrompt, TranscriberConfig};
use crate::global_hotkey::HotkeyHandle;
use crate::messages::{AppEvent, AppEventKind, AppEventSource, UiUpdate};
use futures::SinkExt;
use iced::font::Family;
use iced::theme::Palette;
use iced::widget::{
    Column, button, column, container, pick_list, row, scrollable, text, text_editor, text_input,
    toggler,
};
use iced::{
    Background, Border, Color, Element, Fill, FillPortion, Font, Subscription, Task, Theme,
    keyboard, overlay, time, window,
};
use log::debug;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Hides the application and returns focus to the previously active app.
#[cfg(target_os = "macos")]
fn hide_app() {
    use objc2_app_kit::NSApplication;
    // Safety: iced runs the UI on the main thread.
    let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.hide(None);
}

/// Un-hides the application and brings it to the front.
#[cfg(target_os = "macos")]
fn show_app() {
    use objc2_app_kit::NSApplication;
    // Safety: iced runs the UI on the main thread.
    let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.unhide(None);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

#[cfg(not(target_os = "macos"))]
fn hide_app() {}

#[cfg(not(target_os = "macos"))]
fn show_app() {}

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
        snap: false,
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
        snap: false,
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
        snap: false,
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
        snap: false,
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
        snap: false,
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
        snap: false,
    }
}

// Carousel chip — selected state
fn carousel_chip_active(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgba(0.976, 0.361, 0.576, 0.18),
        button::Status::Pressed => Color::from_rgba(0.976, 0.361, 0.576, 0.28),
        _ => Color::from_rgba(0.976, 0.361, 0.576, 0.12),
    };
    button::Style {
        text_color: PINK,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::from_rgba(0.976, 0.361, 0.576, 0.3),
            width: 1.0,
            radius: 14.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}

// Carousel chip — inactive state
fn carousel_chip_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg, text_color) = match status {
        button::Status::Hovered => (Color::from_rgba(1.0, 1.0, 1.0, 0.06), TEXT_COLOR),
        button::Status::Pressed => (Color::from_rgba(1.0, 1.0, 1.0, 0.10), TEXT_COLOR),
        _ => (Color::TRANSPARENT, MUTED),
    };
    button::Style {
        text_color,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 14.0.into(),
        },
        shadow: Default::default(),
        snap: false,
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
        snap: false,
    }
}

fn hotkey_input(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.08),
        _ => Color::from_rgba(1.0, 1.0, 1.0, 0.04),
    };
    button::Style {
        text_color: TEXT_COLOR,
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color::from_rgb(0.22, 0.23, 0.27),
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}

fn hotkey_input_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        text_color: PINK,
        background: Some(Background::Color(Color::from_rgba(
            0.976, 0.361, 0.576, 0.08,
        ))),
        border: Border {
            color: PINK,
            width: 1.5,
            radius: 8.0.into(),
        },
        shadow: Default::default(),
        snap: false,
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
        snap: false,
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
        snap: false,
    }
}

// ── Style: text input / editor ───────────────────────────────────────
fn borderless_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => PINK,
        _ => Color::TRANSPARENT,
    };
    text_input::Style {
        background: Background::Color(SURFACE),
        border: Border {
            color: border_color,
            width: if matches!(status, text_input::Status::Focused { .. }) {
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

fn styled_pick_list(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border_color = match status {
        pick_list::Status::Opened { .. } => PINK,
        pick_list::Status::Hovered => Color::from_rgba(0.976, 0.361, 0.576, 0.4),
        _ => Color::TRANSPARENT,
    };
    pick_list::Style {
        background: Background::Color(SURFACE),
        text_color: TEXT_COLOR,
        placeholder_color: MUTED,
        handle_color: MUTED,
        border: Border {
            color: border_color,
            width: if matches!(status, pick_list::Status::Opened { .. }) {
                1.0
            } else {
                0.0
            },
            radius: 8.0.into(),
        },
    }
}

fn pick_list_menu(_theme: &Theme) -> overlay::menu::Style {
    overlay::menu::Style {
        background: Background::Color(SURFACE),
        text_color: TEXT_COLOR,
        selected_text_color: Color::WHITE,
        selected_background: Background::Color(PINK),
        border: Border {
            color: MUTED,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Default::default(),
    }
}

fn borderless_editor(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused { .. } => PINK,
        _ => Color::TRANSPARENT,
    };
    text_editor::Style {
        background: Background::Color(SURFACE),
        border: Border {
            color: border_color,
            width: if matches!(status, text_editor::Status::Focused { .. }) {
                1.0
            } else {
                0.0
            },
            radius: 8.0.into(),
        },
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
    /// Recording stopped; reconciling full audio through Whisper.
    Reconciling,
    Processing,
}

/// Controls whether the app shows the setup wizard or the main UI.
#[derive(Clone, Debug, Default, PartialEq)]
enum AppPhase {
    /// First-launch wizard — model must be configured before proceeding.
    #[default]
    Setup,
    /// Normal operation — model is configured and transcriber is running.
    Main,
}

const MAX_PROMPTS: usize = 10;

struct SetupFields {
    model_path: String,
    window_secs: String,
    overlap_secs: String,
    silence_thresh: String,
    input_devices: Vec<String>,
    selected_input_device: Option<String>,
    global_hotkey: String,
    hotkey_listening: bool,
}

#[derive(Clone, Debug, Default)]
struct PromptEntry {
    name: String,
    instruction: String,
}

#[derive(Clone)]
pub struct Ui {
    app_event_tx: AppEventSender,
    hotkey_handle: Option<Arc<Mutex<HotkeyHandle>>>,
    ui_update_rx: Arc<Mutex<Option<UiUpdateReceiver>>>,
    model_exists: bool,
}

impl Ui {
    pub fn new(
        app_event_tx: AppEventSender,
        hotkey_handle: Option<HotkeyHandle>,
        ui_update_rx: UiUpdateReceiver,
        model_exists: bool,
    ) -> Self {
        Self {
            app_event_tx,
            hotkey_handle: hotkey_handle.map(|h| Arc::new(Mutex::new(h))),
            ui_update_rx: Arc::new(Mutex::new(Some(ui_update_rx))),
            model_exists,
        }
    }

    pub fn run(self) -> iced::Result {
        let app_event_tx = self.app_event_tx;
        let hotkey_handle = self.hotkey_handle;
        let ui_update_rx = self.ui_update_rx;
        let model_exists = self.model_exists;
        let boot = move || {
            (
                UiRuntime {
                    app_event_tx: app_event_tx.clone(),
                    hotkey_handle: hotkey_handle.clone(),
                    ui_update_rx: ui_update_rx.clone(),
                    editor: text_editor::Content::new(),
                    status_line: "Ready".to_string(),
                    instruction_editors: Vec::new(),
                    window_id: None,
                    pulse_phase: 0.0,
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
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
                    config_input_devices: Vec::new(),
                    config_selected_input_device: None,
                    config_global_hotkey: String::new(),
                    config_hotkey_listening: false,
                    config_use_gpu: true,
                    config_flash_attn: true,
                    config_no_timestamps: true,
                    config_tab: ConfigTab::default(),
                    active_prompt: 0,
                    snapshot_prompts: Vec::new(),
                    snapshot_default: 0,
                    snapshot_transcriber: None,
                    snapshot_selected_input_device: None,
                    snapshot_global_hotkey: String::new(),
                    phase: if model_exists {
                        AppPhase::Main
                    } else {
                        AppPhase::Setup
                    },
                    wizard_selected_model: 2,
                    wizard_download_progress: None,
                    wizard_downloading: false,
                    wizard_error: None,
                    wizard_cancel_flag: Arc::new(AtomicBool::new(false)),
                    wizard_from_settings: false,
                },
                Task::none(),
            )
        };
        iced::application(boot, update, view)
            .title("Arai")
            .theme(theme)
            .subscription(subscription)
            .window_size((480.0, 620.0))
            .decorations(false)
            .resizable(false)
            .font(include_bytes!("../assets/fonts/MaterialIcons-Regular.ttf").as_slice())
            .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
            .run()
    }
}

const UNDO_LIMIT: usize = 100;

struct UiRuntime {
    app_event_tx: AppEventSender,
    hotkey_handle: Option<Arc<Mutex<HotkeyHandle>>>,
    ui_update_rx: Arc<Mutex<Option<UiUpdateReceiver>>>,
    editor: text_editor::Content,
    status_line: String,
    instruction_editors: Vec<text_editor::Content>,
    window_id: Option<window::Id>,
    /// Pulse phase in radians for the processing indicator animation.
    pulse_phase: f32,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
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
    config_input_devices: Vec<String>,
    config_selected_input_device: Option<String>,
    config_global_hotkey: String,
    config_hotkey_listening: bool,
    config_use_gpu: bool,
    config_flash_attn: bool,
    config_no_timestamps: bool,
    config_tab: ConfigTab,
    /// Index of the prompt currently selected in the main view carousel.
    active_prompt: usize,
    snapshot_prompts: Vec<AgentPrompt>,
    snapshot_default: usize,
    snapshot_transcriber: Option<TranscriberConfig>,
    snapshot_selected_input_device: Option<String>,
    snapshot_global_hotkey: String,
    /// Current app phase — Setup wizard or Main UI.
    #[allow(dead_code)]
    phase: AppPhase,
    /// Index of the selected model in the wizard's download list.
    #[allow(dead_code)]
    wizard_selected_model: usize,
    /// Download progress: (bytes_downloaded, total_bytes). None if not downloading.
    #[allow(dead_code)]
    wizard_download_progress: Option<(u64, u64)>,
    /// Whether a download is currently in progress.
    #[allow(dead_code)]
    wizard_downloading: bool,
    /// Error message to display in the wizard, if any.
    #[allow(dead_code)]
    wizard_error: Option<String>,
    /// Cancel flag for the download thread.
    #[allow(dead_code)]
    wizard_cancel_flag: Arc<AtomicBool>,
    /// Whether the wizard was opened from settings (shows Cancel/Back button).
    #[allow(dead_code)]
    wizard_from_settings: bool,
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
    BrowseModelPath,
    ModelPathPicked(Option<String>),
    WindowSecondsChanged(String),
    OverlapSecondsChanged(String),
    SilenceThresholdChanged(String),
    InputDeviceSelected(String),
    StartHotkeyCapture,
    HotkeyCaptured(String),
    Undo,
    Redo,
    SelectActivePrompt(usize),
    UseGpuToggled(bool),
    FlashAttnToggled(bool),
    NoTimestampsToggled(bool),
    SwitchConfigTab(ConfigTab),
    #[allow(dead_code)]
    WizardSelectModel(usize),
    #[allow(dead_code)]
    WizardStartDownload,
    #[allow(dead_code)]
    WizardCancelDownload,
    #[allow(dead_code)]
    WizardBrowseModel,
    #[allow(dead_code)]
    WizardModelPicked(Option<String>),
    #[allow(dead_code)]
    WizardDownloadProgress(u64, u64),
    #[allow(dead_code)]
    WizardDownloadComplete(std::path::PathBuf),
    #[allow(dead_code)]
    WizardDownloadFailed(String),
    #[allow(dead_code)]
    WizardDownloadCancelled,
    #[allow(dead_code)]
    WizardBack,
    #[allow(dead_code)]
    OpenWizardFromSettings,
    Shutdown,
    KeyPressed(keyboard::Key, keyboard::Modifiers),
    WindowOpened(window::Id),
    DragWindow,
}

impl UiRuntime {
    fn send_event(&self, kind: AppEventKind) {
        let _ = self.app_event_tx.send(AppEvent {
            source: AppEventSource::Ui,
            kind,
        });
    }

    fn toggle_listen(&mut self) {
        if self.mode == AppMode::Processing || self.mode == AppMode::Reconciling {
            return;
        }
        if self.mode == AppMode::Listening {
            debug!("UI stopping listen");
            self.send_event(AppEventKind::UiStopListening);
            self.mode = AppMode::Reconciling;
            self.status_line = "Reconciling...".to_string();
            play_blip();
        } else {
            debug!("UI starting listen");
            self.send_event(AppEventKind::UiStartListening(self.input.clone()));
            self.mode = AppMode::Listening;
            self.status_line = "Listening...".to_string();
            play_blip();
        }
    }

    fn submit(&mut self) {
        if self.mode != AppMode::Idle || self.input.trim().is_empty() {
            return;
        }
        let instruction = self
            .snapshot_prompts
            .get(self.active_prompt)
            .map(|p| p.instruction.clone())
            .unwrap_or_default();
        self.mode = AppMode::Processing;
        self.processed_text = None;
        self.status_line = "Processing...".to_string();
        debug!("UI submit requested");
        self.send_event(AppEventKind::UiSubmitText {
            text: self.input.clone(),
            instruction,
        });
    }
}

fn update(state: &mut UiRuntime, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            // Poll global hotkey — toggle listen and focus window on press.
            let hotkey_fired = if let Some(ref handle) = state.hotkey_handle
                && handle.lock().unwrap().poll_event()
            {
                state.toggle_listen();
                true
            } else {
                false
            };

            // Advance pulse animation while processing or reconciling (~2.4 Hz cycle at 16ms ticks).
            if matches!(state.mode, AppMode::Processing | AppMode::Reconciling) {
                state.pulse_phase += 0.15;
            } else {
                state.pulse_phase = 0.0;
            }

            if hotkey_fired {
                show_app();
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
                        // Append only the new portion to the editor instead of
                        // recreating it, which avoids cursor resets and dropped
                        // updates from rapid Content::with_text() calls.
                        let delta = if text.starts_with(&state.input) {
                            &text[state.input.len()..]
                        } else {
                            // Full text diverged (e.g. after user edit) — replace.
                            state.editor = text_editor::Content::with_text(&text);
                            state.input = text;
                            state.status_line = "Listening...".to_string();
                            return Task::none();
                        };
                        if !delta.is_empty() {
                            // Move cursor to end, then insert the delta.
                            state.editor.perform(text_editor::Action::Move(
                                text_editor::Motion::DocumentEnd,
                            ));
                            for ch in delta.chars() {
                                state.editor.perform(text_editor::Action::Edit(
                                    text_editor::Edit::Insert(ch),
                                ));
                            }
                        }
                        state.input = text;
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
                UiUpdate::ReconciliationStarted => {
                    state.mode = AppMode::Reconciling;
                    state.status_line = "Reconciling...".to_string();
                }
                UiUpdate::ReconciliationComplete(text) => {
                    if !text.is_empty() {
                        state.input = text;
                        state.editor = text_editor::Content::with_text(&state.input);
                    }
                    state.mode = AppMode::Idle;
                    state.status_line = "Ready".to_string();
                }
                UiUpdate::ConfigSnapshot {
                    agent_prompts,
                    default_prompt,
                    transcriber,
                    selected_input_device,
                    global_hotkey,
                } => {
                    // Sync active_prompt to default when prompts change.
                    if state.active_prompt >= agent_prompts.len() {
                        state.active_prompt = default_prompt;
                    }
                    state.snapshot_prompts = agent_prompts;
                    state.snapshot_default = default_prompt;
                    state.snapshot_transcriber = Some(transcriber);
                    state.snapshot_selected_input_device = selected_input_device;
                    state.snapshot_global_hotkey = global_hotkey;
                }
                UiUpdate::ModelDownloadProgress(_, _) => {}
                UiUpdate::ModelDownloadComplete(_) => {}
                UiUpdate::ModelDownloadFailed(_) => {}
                UiUpdate::ModelDownloadCancelled => {}
            }
            Task::none()
        }
        Message::EditorAction(action) => {
            let is_edit = action.is_edit();
            if is_edit {
                state.undo_stack.push(state.input.clone());
                if state.undo_stack.len() > UNDO_LIMIT {
                    state.undo_stack.remove(0);
                }
                state.redo_stack.clear();
            }
            state.editor.perform(action);
            if state.mode == AppMode::Idle {
                state.input = state.editor.text();
            }
            Task::none()
        }
        Message::Undo => {
            if let Some(text) = state.undo_stack.pop() {
                let pos = state.editor.cursor().position;
                state.redo_stack.push(state.input.clone());
                state.input = text;
                state.editor = text_editor::Content::with_text(&state.input);
                restore_cursor(&mut state.editor, pos.line, pos.column);
            }
            Task::none()
        }
        Message::Redo => {
            if let Some(text) = state.redo_stack.pop() {
                let pos = state.editor.cursor().position;
                state.undo_stack.push(state.input.clone());
                state.input = text;
                state.editor = text_editor::Content::with_text(&state.input);
                restore_cursor(&mut state.editor, pos.line, pos.column);
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
            let text = state.input.clone();
            state.input.clear();
            state.editor = text_editor::Content::new();
            hide_app();
            iced::clipboard::write::<Message>(text)
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
            state.config_use_gpu = tc.use_gpu;
            state.config_flash_attn = tc.flash_attn;
            state.config_no_timestamps = tc.no_timestamps;
            state.config_input_devices = crate::recorder::Recorder::list_input_devices();
            state.config_selected_input_device = state.snapshot_selected_input_device.clone();
            state.config_global_hotkey = state.snapshot_global_hotkey.clone();
            state.config_hotkey_listening = false;
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
                use_gpu: state.config_use_gpu,
                flash_attn: state.config_flash_attn,
                no_timestamps: state.config_no_timestamps,
            }));

            state.send_event(AppEventKind::UiUpdateInputDevice(
                state.config_selected_input_device.clone(),
            ));

            // Re-register global hotkey if it changed.
            if state.config_global_hotkey != state.snapshot_global_hotkey
                && !state.config_global_hotkey.is_empty()
            {
                if let Some(ref handle) = state.hotkey_handle {
                    let mut guard = handle.lock().unwrap();
                    guard.re_register(&state.config_global_hotkey);
                }
                state.send_event(AppEventKind::UiUpdateGlobalHotkey(
                    state.config_global_hotkey.clone(),
                ));
            }

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
        Message::BrowseModelPath => Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title("Select Whisper Model")
                    .add_filter("GGML Model", &["bin"])
                    .pick_file()
                    .await;
                handle.map(|h| h.path().to_string_lossy().into_owned())
            },
            Message::ModelPathPicked,
        ),
        Message::ModelPathPicked(path) => {
            if let Some(path) = path {
                state.config_model_path = path;
            }
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
        Message::SelectActivePrompt(idx) => {
            if idx < state.snapshot_prompts.len() {
                state.active_prompt = idx;
            }
            Task::none()
        }
        Message::InputDeviceSelected(value) => {
            state.config_selected_input_device = Some(value);
            Task::none()
        }
        Message::UseGpuToggled(value) => {
            state.config_use_gpu = value;
            Task::none()
        }
        Message::FlashAttnToggled(value) => {
            state.config_flash_attn = value;
            Task::none()
        }
        Message::NoTimestampsToggled(value) => {
            state.config_no_timestamps = value;
            Task::none()
        }
        Message::StartHotkeyCapture => {
            state.config_hotkey_listening = true;
            Task::none()
        }
        Message::HotkeyCaptured(hotkey_str) => {
            state.config_global_hotkey = hotkey_str;
            state.config_hotkey_listening = false;
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
        Message::WizardSelectModel(_)
        | Message::WizardStartDownload
        | Message::WizardCancelDownload
        | Message::WizardDownloadProgress(_, _)
        | Message::WizardDownloadComplete(_)
        | Message::WizardDownloadFailed(_)
        | Message::WizardDownloadCancelled
        | Message::WizardBack
        | Message::OpenWizardFromSettings => Task::none(),
        Message::WizardBrowseModel => Task::none(),
        Message::WizardModelPicked(_) => Task::none(),
        Message::DragWindow => {
            if let Some(id) = state.window_id {
                window::drag(id)
            } else {
                Task::none()
            }
        }
        Message::WindowOpened(id) => {
            state.window_id = Some(id);
            match load_window_icon() {
                Some(icon) => window::set_icon(id, icon),
                None => Task::none(),
            }
        }
        Message::KeyPressed(key, modifiers) => {
            // Intercept keypresses for global hotkey capture mode.
            if state.config_hotkey_listening {
                if let Some(hotkey_str) = iced_key_to_hotkey_string(&key, &modifiers) {
                    return update(state, Message::HotkeyCaptured(hotkey_str));
                }
                // Ignore modifier-only presses, wait for a full combo.
                return Task::none();
            }
            match key {
                keyboard::Key::Named(keyboard::key::Named::Enter) if modifiers.shift() => {
                    update(state, Message::Copy)
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    state.submit();
                    Task::none()
                }
                keyboard::Key::Character(ref c) if c.as_str() == "c" && modifiers.command() => {
                    update(state, Message::Copy)
                }
                keyboard::Key::Character(ref c)
                    if c.as_str() == "z" && modifiers.command() && modifiers.shift() =>
                {
                    update(state, Message::Redo)
                }
                keyboard::Key::Character(ref c)
                    if c.as_str() == "z" && modifiers.command() && !modifiers.shift() =>
                {
                    update(state, Message::Undo)
                }
                keyboard::Key::Character(ref c) if c.as_str() == "w" && modifiers.command() => {
                    hide_app();
                    Task::none()
                }
                keyboard::Key::Character(ref c)
                    if modifiers.command()
                        && c.as_str().len() == 1
                        && c.as_str().as_bytes()[0].is_ascii_digit() =>
                {
                    let digit = c.as_str().as_bytes()[0] - b'0';
                    let idx = if digit == 0 {
                        state.snapshot_prompts.len().saturating_sub(1)
                    } else {
                        (digit as usize).saturating_sub(1)
                    };
                    update(state, Message::SelectActivePrompt(idx))
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    if state.config_hotkey_listening {
                        state.config_hotkey_listening = false;
                    } else if state.config_open {
                        state.config_open = false;
                    }
                    Task::none()
                }
                _ => Task::none(),
            }
        }
    }
}

/// Converts an iced keyboard event into a `global-hotkey` format string
/// (e.g. `"CmdOrCtrl+Shift+A"`). Returns `None` if only modifier keys
/// are pressed without a main key.
fn iced_key_to_hotkey_string(
    key: &keyboard::Key,
    modifiers: &keyboard::Modifiers,
) -> Option<String> {
    let main_key = match key {
        keyboard::Key::Character(c) => {
            let s = c.as_str().to_uppercase();
            if s.is_empty() {
                return None;
            }
            s
        }
        keyboard::Key::Named(named) => {
            use keyboard::key::Named;
            match named {
                Named::Escape => "Escape".to_string(),
                Named::Enter => "Enter".to_string(),
                Named::Tab => "Tab".to_string(),
                Named::Space => "Space".to_string(),
                Named::Backspace => "Backspace".to_string(),
                Named::Delete => "Delete".to_string(),
                Named::ArrowUp => "ArrowUp".to_string(),
                Named::ArrowDown => "ArrowDown".to_string(),
                Named::ArrowLeft => "ArrowLeft".to_string(),
                Named::ArrowRight => "ArrowRight".to_string(),
                Named::Home => "Home".to_string(),
                Named::End => "End".to_string(),
                Named::PageUp => "PageUp".to_string(),
                Named::PageDown => "PageDown".to_string(),
                Named::F1 => "F1".to_string(),
                Named::F2 => "F2".to_string(),
                Named::F3 => "F3".to_string(),
                Named::F4 => "F4".to_string(),
                Named::F5 => "F5".to_string(),
                Named::F6 => "F6".to_string(),
                Named::F7 => "F7".to_string(),
                Named::F8 => "F8".to_string(),
                Named::F9 => "F9".to_string(),
                Named::F10 => "F10".to_string(),
                Named::F11 => "F11".to_string(),
                Named::F12 => "F12".to_string(),
                // Modifier-only presses — ignore them, wait for a real key.
                Named::Shift | Named::Control | Named::Alt | Named::Super => return None,
                _ => return None,
            }
        }
        keyboard::Key::Unidentified => return None,
    };

    let mut parts = Vec::new();
    if modifiers.command() {
        parts.push("CmdOrCtrl");
    }
    if modifiers.shift() {
        parts.push("Shift");
    }
    if modifiers.alt() {
        parts.push("Alt");
    }
    parts.push(&main_key);
    Some(parts.join("+"))
}

/// Embedded blip sound played when recording starts or stops.
static BLIP_WAV: &[u8] = include_bytes!("../assets/sounds/blip.wav");
static LOGO_PNG: &[u8] = include_bytes!("../assets/images/logo.png");

/// Path to the cached blip WAV file written to a temp location on first use.
static BLIP_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Writes the embedded blip WAV to a temp file (once) and returns the path.
fn blip_file_path() -> &'static std::path::Path {
    BLIP_PATH.get_or_init(|| {
        let path = std::env::temp_dir().join("arai-blip.wav");
        if let Err(e) = std::fs::write(&path, BLIP_WAV) {
            log::warn!("Failed to write blip sound to temp: {e}");
        }
        path
    })
}

/// Plays the blip sound on a background thread so it doesn't block the UI.
/// Uses platform audio commands to avoid pulling in a second cpal version
/// and triggering extra macOS permission prompts.
fn play_blip() {
    let path = blip_file_path().to_path_buf();
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("afplay").arg(&path).output();
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("aplay").arg(&path).output();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("powershell")
            .args([
                "-c",
                &format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    path.display()
                ),
            ])
            .output();

        if let Err(e) = result {
            log::warn!("Failed to play blip sound: {e}");
        }
    });
}

/// Decodes the embedded logo PNG and returns an iced window icon.
fn load_window_icon() -> Option<window::Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    window::icon::from_rgba(buf, info.width, info.height).ok()
}

/// Moves the cursor in a freshly-created `Content` to `(line, col)`,
/// clamping to the actual text bounds.
fn restore_cursor(content: &mut text_editor::Content, line: usize, col: usize) {
    let line_count = content.line_count();
    let target_line = line.min(line_count.saturating_sub(1));
    let line_len = content.line(target_line).map(|l| l.text.len()).unwrap_or(0);
    let target_col = col.min(line_len);
    for _ in 0..target_line {
        content.perform(text_editor::Action::Move(text_editor::Motion::Down));
    }
    for _ in 0..target_col {
        content.perform(text_editor::Action::Move(text_editor::Motion::Right));
    }
}

// ── Views ────────────────────────────────────────────────────────────

fn view(state: &UiRuntime) -> Element<'_, Message> {
    let content = if state.config_open {
        let setup_fields = SetupFields {
            model_path: state.config_model_path.clone(),
            window_secs: state.config_window_seconds.clone(),
            overlap_secs: state.config_overlap_seconds.clone(),
            silence_thresh: state.config_silence_threshold.clone(),
            input_devices: state.config_input_devices.clone(),
            selected_input_device: state.config_selected_input_device.clone(),
            global_hotkey: state.config_global_hotkey.clone(),
            hotkey_listening: state.config_hotkey_listening,
        };
        view_config(
            state,
            state.config_prompts.clone(),
            state.config_default,
            setup_fields,
            state.config_tab.clone(),
        )
    } else {
        let listening = state.mode == AppMode::Listening;
        let processing = state.mode == AppMode::Processing;
        let reconciling = state.mode == AppMode::Reconciling;

        view_main(
            state,
            listening,
            processing,
            reconciling,
            !state.input.trim().is_empty(),
            state.input.chars().count(),
        )
    };

    iced::widget::mouse_area(content)
        .on_press(Message::DragWindow)
        .into()
}

fn view_main<'a>(
    state: &'a UiRuntime,
    listening: bool,
    processing: bool,
    reconciling: bool,
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

    let mut editor_widget = text_editor(&state.editor)
        .style(borderless_editor)
        .wrapping(text::Wrapping::Word)
        .padding(16)
        .height(Fill)
        .key_binding(|key_press| match &key_press.key {
            keyboard::Key::Named(keyboard::key::Named::Enter) if key_press.modifiers.shift() => {
                Some(text_editor::Binding::Custom(Message::Copy))
            }
            keyboard::Key::Named(keyboard::key::Named::Enter) if key_press.modifiers.is_empty() => {
                Some(text_editor::Binding::Custom(Message::Submit))
            }
            keyboard::Key::Character(c) if c.as_str() == "z" && key_press.modifiers.command() => {
                if key_press.modifiers.shift() {
                    Some(text_editor::Binding::Custom(Message::Redo))
                } else {
                    Some(text_editor::Binding::Custom(Message::Undo))
                }
            }
            keyboard::Key::Character(c) if c.as_str() == "c" && key_press.modifiers.command() => {
                Some(text_editor::Binding::Custom(Message::Copy))
            }
            _ => text_editor::Binding::from_key_press(key_press),
        });
    let busy = listening || processing || reconciling;
    if !busy {
        editor_widget = editor_widget.on_action(Message::EditorAction);
    }

    let char_count_text = text(format!("{} chars", char_count)).size(12).color(MUTED);

    // mic: E029=mic, E02B=mic_off
    let mic_btn = if listening {
        button(icon('\u{E02B}', 22.0))
            .style(icon_btn_active)
            .padding([8, 12])
            .on_press(Message::ToggleListen)
    } else if reconciling {
        // Pulsate mic icon green while reconciling.
        let t = state.pulse_phase.sin() * 0.5 + 0.5;
        let pulse_color = Color::from_rgb(
            0.651 * t + 0.25 * (1.0 - t),
            0.886 * t + 0.35 * (1.0 - t),
            0.180 * t + 0.10 * (1.0 - t),
        );
        button(icon('\u{E029}', 22.0).color(pulse_color))
            .style(icon_btn)
            .padding([8, 12])
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
            .on_press_maybe((!busy && has_text).then_some(Message::Submit))
    };

    // copy: E14D
    let copy_btn = button(icon('\u{E14D}', 22.0))
        .style(icon_btn)
        .padding([8, 12])
        .on_press_maybe((has_text && !busy).then_some(Message::Copy));

    // settings: E8B8
    let settings_btn = button(icon('\u{E8B8}', 22.0))
        .style(icon_btn)
        .padding([8, 12])
        .on_press_maybe((!busy).then_some(Message::OpenConfig));

    let button_group = row![mic_btn, send_btn, copy_btn, settings_btn]
        .spacing(16)
        .align_y(iced::Alignment::Center);

    let bottom_bar = column![
        container(button_group).center_x(Fill),
        container(char_count_text).padding([4, 18])
    ]
    .spacing(6);

    // ── Prompt carousel ──────────────────────────────────────────────
    let prompt_carousel = {
        let mut chips = row![].spacing(6).align_y(iced::Alignment::Center);
        for (idx, prompt) in state.snapshot_prompts.iter().enumerate() {
            let is_active = idx == state.active_prompt;
            let chip = button(text(&prompt.name).size(12))
                .style(if is_active {
                    carousel_chip_active
                } else {
                    carousel_chip_inactive
                })
                .padding([4, 12])
                .on_press(Message::SelectActivePrompt(idx));
            chips = chips.push(chip);
        }
        container(
            scrollable(chips)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new(),
                ))
                .width(Fill),
        )
        .padding([0, 14])
    };

    let body = column![
        prompt_carousel,
        container(editor_widget)
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
    sf: SetupFields,
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
        ConfigTab::Setup => view_setup_tab(&sf),
        ConfigTab::Instructions => view_instructions_tab(state, &prompts, config_default),
        ConfigTab::Advanced => view_advanced_tab(state),
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

fn view_setup_tab(sf: &SetupFields) -> Column<'static, Message> {
    // ── Microphone card ─────────────────────────────────────────────
    let device_picker = pick_list(
        sf.input_devices.clone(),
        sf.selected_input_device.clone(),
        Message::InputDeviceSelected,
    )
    .placeholder("System Default")
    .style(styled_pick_list)
    .menu_style(pick_list_menu)
    .padding(10)
    .width(Fill);

    let mic_card = column![
        text("Microphone").size(15).color(TEXT_COLOR),
        column![text("Input Device").size(11).color(MUTED), device_picker].spacing(4),
    ]
    .spacing(10)
    .padding(14);

    // ── Transcriber card ────────────────────────────────────────────
    let model_path_input = text_input("Model path", &sf.model_path)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::ModelPathChanged);

    // folder_open: E2C8
    let browse_btn = button(icon('\u{E2C8}', 18.0))
        .style(icon_btn)
        .padding([8, 10])
        .on_press(Message::BrowseModelPath);

    let model_path_row = row![container(model_path_input).width(Fill), browse_btn]
        .spacing(4)
        .align_y(iced::Alignment::Center);

    let window_secs_input = text_input("Window seconds", &sf.window_secs)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::WindowSecondsChanged);

    let overlap_secs_input = text_input("Overlap seconds", &sf.overlap_secs)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::OverlapSecondsChanged);

    let silence_thresh_input = text_input("Silence threshold", &sf.silence_thresh)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::SilenceThresholdChanged);

    let transcriber_card = column![
        text("Transcriber").size(15).color(TEXT_COLOR),
        column![text("Model Path").size(11).color(MUTED), model_path_row].spacing(4),
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

    // ── Global Hotkey card ───────────────────────────────────────────
    let hotkey_display = if sf.hotkey_listening {
        "Press a key combination...".to_string()
    } else if sf.global_hotkey.is_empty() {
        "Not set".to_string()
    } else {
        sf.global_hotkey.clone()
    };

    let hotkey_btn = button(text(hotkey_display).size(13).color(if sf.hotkey_listening {
        PINK
    } else {
        TEXT_COLOR
    }))
    .style(if sf.hotkey_listening {
        hotkey_input_active
    } else {
        hotkey_input
    })
    .padding(10)
    .width(Fill)
    .on_press(Message::StartHotkeyCapture);

    let hotkey_card = column![
        text("Keyboard Shortcut").size(15).color(TEXT_COLOR),
        column![text("Quick Launch").size(11).color(MUTED), hotkey_btn].spacing(4),
    ]
    .spacing(10)
    .padding(14);

    column![
        container(mic_card).style(surface_container).width(Fill),
        container(hotkey_card).style(surface_container).width(Fill),
        container(transcriber_card)
            .style(surface_container)
            .width(Fill),
    ]
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

fn view_advanced_tab(state: &UiRuntime) -> Column<'_, Message> {
    let gpu_toggle = toggler(state.config_use_gpu)
        .label("GPU Acceleration")
        .on_toggle(Message::UseGpuToggled)
        .text_size(13)
        .spacing(10)
        .size(20);

    let flash_attn_toggle = toggler(state.config_flash_attn)
        .label("Flash Attention")
        .on_toggle(Message::FlashAttnToggled)
        .text_size(13)
        .spacing(10)
        .size(20);

    let no_timestamps_toggle = toggler(state.config_no_timestamps)
        .label("Disable Timestamps")
        .on_toggle(Message::NoTimestampsToggled)
        .text_size(13)
        .spacing(10)
        .size(20);

    let gpu_card = column![
        text("Model Inference").size(15).color(TEXT_COLOR),
        column![
            text("Enable Metal GPU for faster inference on Apple Silicon.")
                .size(11)
                .color(MUTED),
            gpu_toggle,
        ]
        .spacing(6),
        column![
            text("Use flash attention for reduced memory and faster decoding.")
                .size(11)
                .color(MUTED),
            flash_attn_toggle,
        ]
        .spacing(6),
        column![
            text("Skip timestamp computation for faster output.")
                .size(11)
                .color(MUTED),
            no_timestamps_toggle,
        ]
        .spacing(6),
    ]
    .spacing(12)
    .padding(14);

    column![container(gpu_card).style(surface_container).width(Fill)]
        .spacing(12)
        .padding(14)
}

fn subscription(state: &UiRuntime) -> Subscription<Message> {
    let ui_update_rx = state.ui_update_rx.clone();
    Subscription::batch([
        time::every(Duration::from_millis(16)).map(|_| Message::Tick),
        keyboard::listen().map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Message::KeyPressed(key, modifiers)
            }
            _ => Message::Tick,
        }),
        window::open_events().map(Message::WindowOpened),
        Subscription::run_with(UiUpdateBridge(ui_update_rx), ui_update_stream),
    ])
}

struct UiUpdateBridge(Arc<Mutex<Option<UiUpdateReceiver>>>);

impl std::hash::Hash for UiUpdateBridge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "ui-update-bridge".hash(state);
    }
}

fn ui_update_stream(
    bridge: &UiUpdateBridge,
) -> std::pin::Pin<Box<dyn futures::Stream<Item = Message> + Send>> {
    let rx = bridge.0.clone();
    Box::pin(iced::stream::channel(
        100,
        move |mut sender: futures::channel::mpsc::Sender<Message>| async move {
            let rx = {
                let mut guard = rx.lock().unwrap();
                guard.take()
            };
            if let Some(rx) = rx {
                std::thread::spawn(move || {
                    while let Ok(update) = rx.recv() {
                        if futures::executor::block_on(
                            sender.send(Message::UiUpdateReceived(update)),
                        )
                        .is_err()
                        {
                            log::warn!("UI channel closed, bridge exiting");
                            break;
                        }
                    }
                });
            }
            std::future::pending::<()>().await;
        },
    ))
}

fn theme(_state: &UiRuntime) -> Theme {
    Theme::custom(
        "Arai".to_string(),
        Palette {
            background: BG,
            text: TEXT_COLOR,
            primary: PINK,
            success: GREEN,
            warning: Color::from_rgb(0.976, 0.659, 0.145),
            danger: RED,
        },
    )
}
