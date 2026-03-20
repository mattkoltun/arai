# Whisper Model Setup Wizard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first-launch setup wizard that downloads or locates a Whisper model, blocking the main UI until a valid model is configured.

**Architecture:** The iced app gains an `AppPhase` enum (`Setup` vs `Main`) that determines which view renders. On launch, if the model path is invalid, the app enters `Setup` phase showing a wizard with download-from-HuggingFace and browse-for-file options. Download runs on a background thread using `reqwest::blocking` with chunked reads, reporting progress via the existing `AppEvent` channel. The wizard is also re-accessible from settings.

**Tech Stack:** Rust 2024, iced 0.14, reqwest (blocking), dirs crate, existing mpsc channel infrastructure.

**Spec:** `docs/superpowers/specs/2026-03-20-whisper-model-wizard-design.md`

**Key design decisions:**
- Uses `std::sync::LazyLock` (Rust 2024 stdlib) instead of `once_cell::sync::Lazy` for `DEFAULT_MODEL_PATH`.
- Config save and transcriber restart happen ONLY in the controller (via `UiUpdateTranscriber` event) — never duplicated in the UI.
- Browse-for-model validates the file on a background thread before accepting it.
- Progress updates are throttled to ~10/sec to avoid flooding the event channel.

---

### Task 1: Add `dirs` dependency and `default_model_dir()` helper

**Files:**
- Modify: `Cargo.toml:18` (add dirs dependency)
- Modify: `src/config.rs:1-11` (add helper, update default)

- [ ] **Step 1: Add `dirs` to Cargo.toml**

Add after the `futures` line in `Cargo.toml`:
```toml
dirs = "6"
```

- [ ] **Step 2: Implement `default_model_dir()` and update `DEFAULT_MODEL_PATH`**

In `src/config.rs`, replace:
```rust
const DEFAULT_MODEL_PATH: &str = "models/ggml-small.en.bin";
```

With:
```rust
use std::sync::LazyLock;

/// Returns the platform-standard directory for storing Whisper models.
/// - macOS: `~/Library/Application Support/arai/models/`
/// - Linux: `~/.local/share/arai/models/`
pub fn default_model_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arai")
        .join("models")
}

static DEFAULT_MODEL_PATH: LazyLock<String> = LazyLock::new(|| {
    default_model_dir()
        .join("ggml-small.en.bin")
        .display()
        .to_string()
});
```

Update `TranscriberConfig::default()` to use `DEFAULT_MODEL_PATH.clone()`:
```rust
impl Default for TranscriberConfig {
    fn default() -> Self {
        Self {
            model_path: DEFAULT_MODEL_PATH.clone(),
            // ... rest unchanged
        }
    }
}
```

- [ ] **Step 3: Write tests and run them**

In `src/config.rs`, add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn default_model_dir_ends_with_arai_models() {
    let dir = default_model_dir();
    assert!(dir.ends_with("arai/models"), "expected path ending with arai/models, got: {dir:?}");
}

#[test]
fn default_model_path_is_absolute() {
    let path = std::path::Path::new(DEFAULT_MODEL_PATH.as_str());
    assert!(path.is_absolute(), "DEFAULT_MODEL_PATH should be absolute, got: {path:?}");
}
```

Run: `cargo test config::tests -- --nocapture`
Expected: All config tests PASS.

- [ ] **Step 4: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/config.rs
git commit -m "feat: add default_model_dir() and update DEFAULT_MODEL_PATH to platform data dir"
```

---

### Task 2: Make API key optional at config load time

**Files:**
- Modify: `src/config.rs:14-55` (remove MissingApiKey error from `from_partial`)
- Modify: `src/config.rs:287-348` (update tests)

Currently `Config::load()` fails if no API key is set, which would prevent the wizard from ever appearing on a fresh install. The API key is only needed when submitting to the agent, not at startup.

- [ ] **Step 1: Write failing test**

In `src/config.rs` tests:

```rust
#[test]
fn builds_config_without_api_key() {
    let mut partial = valid_partial();
    partial.open_api_key = None;
    let cfg = from_partial(partial).expect("config should load without API key");
    assert!(cfg.open_api_key.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config::tests::builds_config_without_api_key -- --nocapture`
Expected: FAIL — currently returns `Err(ConfigError::MissingApiKey)`.

- [ ] **Step 3: Update `from_partial()` to allow missing API key**

In `src/config.rs`, change the `open_api_key` handling in `from_partial()` from:

```rust
let open_api_key = partial.open_api_key.unwrap_or_default();
if open_api_key.trim().is_empty() {
    return Err(ConfigError::MissingApiKey);
}
```

To:

```rust
let open_api_key = partial.open_api_key.unwrap_or_default();
```

Remove the `MissingApiKey` variant from `ConfigError` and its `Display` impl arm. Also remove the `rejects_missing_api_key` test since that behavior is intentionally removed.

- [ ] **Step 4: Run all config tests**

Run: `cargo test config::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs
git commit -m "feat: make API key optional at config load time for first-launch wizard"
```

---

### Task 3: Add model download event types to messages

**Files:**
- Modify: `src/messages.rs:43-69` (add new AppEventKind variants)
- Modify: `src/messages.rs:13-33` (add new UiUpdate variants)
- Modify: `src/ui.rs` (add stub match arms for new UiUpdate variants)

**Important:** New enum variants must have corresponding match arms added in the same task to avoid breaking the exhaustive `match` in `ui.rs`.

- [ ] **Step 1: Add new `AppEventKind` variants**

In `src/messages.rs`, add to the `AppEventKind` enum:

```rust
/// Model download progress: (bytes_downloaded, total_bytes).
ModelDownloadProgress(u64, u64),
/// Model download completed successfully; carries the path to the downloaded file.
ModelDownloadComplete(std::path::PathBuf),
/// Model download failed with an error message.
ModelDownloadFailed(String),
/// Model download was cancelled by the user.
ModelDownloadCancelled,
```

- [ ] **Step 2: Add new `UiUpdate` variants**

In `src/messages.rs`, add to the `UiUpdate` enum:

```rust
/// Model download progress update for the wizard.
ModelDownloadProgress(u64, u64),
/// Model download completed — carries the saved model path.
ModelDownloadComplete(std::path::PathBuf),
/// Model download failed.
ModelDownloadFailed(String),
/// Model download was cancelled.
ModelDownloadCancelled,
```

- [ ] **Step 3: Add stub match arms in `ui.rs` for new `UiUpdate` variants**

In `src/ui.rs`, inside the `Message::UiUpdateReceived(update)` match (around line 676), add these stubs so the code compiles:

```rust
UiUpdate::ModelDownloadProgress(_, _) => {}
UiUpdate::ModelDownloadComplete(_) => {}
UiUpdate::ModelDownloadFailed(_) => {}
UiUpdate::ModelDownloadCancelled => {}
```

These stubs will be replaced with real handlers in Task 7.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/messages.rs src/ui.rs
git commit -m "feat: add model download event types to messages"
```

---

### Task 4: Add model download module

**Files:**
- Create: `src/model_downloader.rs`
- Modify: `src/main.rs:1` (add `mod model_downloader;`)

This module handles downloading Whisper models from Hugging Face on a background thread, reporting progress via the AppEvent channel. Progress updates are throttled to avoid flooding the channel (~10 updates/sec).

- [ ] **Step 1: Create `src/model_downloader.rs`**

```rust
use crate::channels::AppEventSender;
use crate::config::default_model_dir;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::{error, info};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Available Whisper model variants.
#[derive(Clone, Debug, PartialEq)]
pub struct WhisperModel {
    pub name: &'static str,
    pub file: &'static str,
    pub size_label: &'static str,
    pub description: &'static str,
}

pub const WHISPER_MODELS: &[WhisperModel] = &[
    WhisperModel {
        name: "Tiny (English)",
        file: "ggml-tiny.en.bin",
        size_label: "~75 MB",
        description: "Fastest, least accurate",
    },
    WhisperModel {
        name: "Base (English)",
        file: "ggml-base.en.bin",
        size_label: "~142 MB",
        description: "Fast, decent accuracy",
    },
    WhisperModel {
        name: "Small (English)",
        file: "ggml-small.en.bin",
        size_label: "~487 MB",
        description: "Good balance (recommended)",
    },
    WhisperModel {
        name: "Medium (English)",
        file: "ggml-medium.en.bin",
        size_label: "~1.5 GB",
        description: "High accuracy, slower",
    },
    WhisperModel {
        name: "Large",
        file: "ggml-large-v3-turbo.bin",
        size_label: "~1.5 GB",
        description: "Best accuracy, multilingual",
    },
];

const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Returns the full download URL for a model file.
fn download_url(file: &str) -> String {
    format!("{HF_BASE_URL}/{file}")
}

/// Downloads a Whisper model on a background thread. Progress and
/// completion/failure events are sent via `app_event_tx`. Set `cancel_flag`
/// to `true` from another thread to abort the download. The `.part` file is
/// cleaned up on cancel.
pub fn start_download(
    model: &WhisperModel,
    app_event_tx: AppEventSender,
    cancel_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let file = model.file.to_string();
    std::thread::spawn(move || {
        if let Err(e) = run_download(&file, &app_event_tx, &cancel_flag) {
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Ui,
                    kind: AppEventKind::ModelDownloadCancelled,
                });
            } else {
                error!("Model download failed: {e}");
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Ui,
                    kind: AppEventKind::ModelDownloadFailed(e),
                });
            }
        }
    })
}

fn run_download(
    file: &str,
    app_event_tx: &AppEventSender,
    cancel_flag: &AtomicBool,
) -> Result<(), String> {
    let dest_dir = default_model_dir();
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("Failed to create model directory: {e}"))?;

    let dest_path = dest_dir.join(file);
    let part_path = dest_dir.join(format!("{file}.part"));

    let url = download_url(file);
    info!("Downloading model from {url}");

    let response = reqwest::blocking::Client::new()
        .get(&url)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut reader = response;
    let mut out_file = std::fs::File::create(&part_path)
        .map_err(|e| format!("Failed to create file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks
    let mut last_progress = Instant::now();

    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            drop(out_file);
            let _ = std::fs::remove_file(&part_path);
            return Err("Cancelled".to_string());
        }

        let bytes_read = std::io::Read::read(&mut reader, &mut buf)
            .map_err(|e| format!("Read error: {e}"))?;

        if bytes_read == 0 {
            break;
        }

        out_file
            .write_all(&buf[..bytes_read])
            .map_err(|e| format!("Write error: {e}"))?;

        downloaded += bytes_read as u64;

        // Throttle progress updates to ~10/sec to avoid flooding the channel.
        if last_progress.elapsed().as_millis() >= 100 {
            last_progress = Instant::now();
            let _ = app_event_tx.send(AppEvent {
                source: AppEventSource::Ui,
                kind: AppEventKind::ModelDownloadProgress(downloaded, total_bytes),
            });
        }
    }

    // Send final progress update.
    let _ = app_event_tx.send(AppEvent {
        source: AppEventSource::Ui,
        kind: AppEventKind::ModelDownloadProgress(downloaded, total_bytes),
    });

    out_file.flush().map_err(|e| format!("Flush error: {e}"))?;
    drop(out_file);

    // Atomic rename from .part to final path.
    std::fs::rename(&part_path, &dest_path)
        .map_err(|e| format!("Failed to rename downloaded file: {e}"))?;

    info!("Model downloaded to {}", dest_path.display());
    let _ = app_event_tx.send(AppEvent {
        source: AppEventSource::Ui,
        kind: AppEventKind::ModelDownloadComplete(dest_path),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_models_has_five_entries() {
        assert_eq!(WHISPER_MODELS.len(), 5);
    }

    #[test]
    fn download_url_format_is_correct() {
        let url = download_url("ggml-small.en.bin");
        assert_eq!(
            url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
        );
    }

    #[test]
    fn all_models_have_bin_extension() {
        for model in WHISPER_MODELS {
            assert!(
                model.file.ends_with(".bin"),
                "model {} missing .bin extension",
                model.name
            );
        }
    }
}
```

- [ ] **Step 2: Add module declaration to `main.rs`**

Add `mod model_downloader;` to `src/main.rs` (after `mod messages;`).

- [ ] **Step 3: Run tests**

Run: `cargo test model_downloader::tests -- --nocapture`
Expected: All 3 tests PASS.

- [ ] **Step 4: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add src/model_downloader.rs src/main.rs
git commit -m "feat: add model_downloader module with HuggingFace download support"
```

---

### Task 5: Add `AppPhase` enum, wizard state, messages, and stub handlers to the UI

**Files:**
- Modify: `src/ui.rs` (add AppPhase, wizard state fields, Message variants, stub match arms)
- Modify: `src/main.rs` (pass model_exists, conditional transcriber start)

**Important:** This task adds ALL new Message variants AND their match arms together so the exhaustive match always compiles. The match arms are stubs that return `Task::none()` — real logic comes in Task 7.

- [ ] **Step 1: Add `AppPhase` enum to `ui.rs`**

After the `AppMode` enum (around line 418), add:

```rust
/// Controls whether the app shows the setup wizard or the main UI.
#[derive(Clone, Debug, Default, PartialEq)]
enum AppPhase {
    /// First-launch wizard — model must be configured before proceeding.
    #[default]
    Setup,
    /// Normal operation — model is configured and transcriber is running.
    Main,
}
```

- [ ] **Step 2: Add wizard state fields to `UiRuntime`**

Add these fields to the `UiRuntime` struct:

```rust
/// Current app phase — Setup wizard or Main UI.
phase: AppPhase,
/// Index of the selected model in the wizard's download list.
wizard_selected_model: usize,
/// Download progress: (bytes_downloaded, total_bytes). None if not downloading.
wizard_download_progress: Option<(u64, u64)>,
/// Whether a download is currently in progress.
wizard_downloading: bool,
/// Error message to display in the wizard, if any.
wizard_error: Option<String>,
/// Cancel flag for the download thread.
wizard_cancel_flag: Arc<AtomicBool>,
/// Whether the wizard was opened from settings (shows Cancel/Back button).
wizard_from_settings: bool,
```

Add `model_exists: bool` field to the `Ui` struct. Update `Ui::new()`:
```rust
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
```

In the `boot` closure, initialize wizard fields:
```rust
phase: if model_exists { AppPhase::Main } else { AppPhase::Setup },
wizard_selected_model: 2, // Default to "Small (English)" which is recommended
wizard_download_progress: None,
wizard_downloading: false,
wizard_error: None,
wizard_cancel_flag: Arc::new(AtomicBool::new(false)),
wizard_from_settings: false,
```

Add import at the top of `ui.rs`:
```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

- [ ] **Step 3: Add wizard `Message` variants with stub match arms**

Add to the `Message` enum:

```rust
/// Wizard: user selected a model from the download list.
WizardSelectModel(usize),
/// Wizard: user clicked Download.
WizardStartDownload,
/// Wizard: user clicked Cancel download.
WizardCancelDownload,
/// Wizard: user clicked Browse to pick an existing model.
WizardBrowseModel,
/// Wizard: file dialog returned a path (or None if cancelled).
WizardModelPicked(Option<String>),
/// Wizard: download progress update from background thread.
WizardDownloadProgress(u64, u64),
/// Wizard: download completed successfully.
WizardDownloadComplete(std::path::PathBuf),
/// Wizard: download failed.
WizardDownloadFailed(String),
/// Wizard: download was cancelled.
WizardDownloadCancelled,
/// Wizard: go back to main UI (only from settings).
WizardBack,
/// Open the model setup wizard from settings.
OpenWizardFromSettings,
```

Add stub match arms in `update()` so it compiles:

```rust
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
```

- [ ] **Step 4: Update `main.rs`**

After config is loaded, check if the model file exists and conditionally start transcriber:

```rust
let model_exists = std::path::Path::new(&config.transcriber.model_path).exists();
```

Change the transcriber start to be conditional:
```rust
if model_exists {
    if let Err(err) = transcriber.start() {
        eprintln!("Transcriber failed to start: {err}");
        return;
    }
}
```

Pass `model_exists` to `Ui::new()`:
```rust
let ui = ui::Ui::new(app_event_tx.clone(), hotkey_handle, ui_update_rx, model_exists);
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Build succeeds.

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs src/main.rs
git commit -m "feat: add AppPhase enum, wizard state, and message stubs to UI"
```

---

### Task 6: Implement wizard view rendering

**Files:**
- Modify: `src/ui.rs` (add `view_wizard()` function, update `view()` to route by phase)

- [ ] **Step 1: Update `view()` to route by phase**

Change the `view()` function to check `state.phase` first:

```rust
fn view(state: &UiRuntime) -> Element<'_, Message> {
    let content = match state.phase {
        AppPhase::Setup => view_wizard(state),
        AppPhase::Main => {
            if state.config_open {
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
            }
        }
    };

    iced::widget::mouse_area(content)
        .on_press(Message::DragWindow)
        .into()
}
```

- [ ] **Step 2: Implement `view_wizard()`**

Add the wizard view function. This renders within the existing 480x620 window using the same Tokyo Night theme:

```rust
fn view_wizard(state: &UiRuntime) -> Element<'_, Message> {
    use iced::widget::progress_bar;

    // Close button (top-right)
    let close_btn = button(icon('\u{E5CD}', 20.0))
        .style(icon_btn)
        .padding(6)
        .on_press(Message::Shutdown);

    let mut top_row = row![].align_y(iced::Alignment::Center);
    if state.wizard_from_settings {
        // arrow_back: E5C4
        let back_btn = button(icon('\u{E5C4}', 20.0))
            .style(icon_btn)
            .padding(6)
            .on_press(Message::WizardBack);
        top_row = top_row.push(back_btn);
    }
    top_row = top_row.push(container(close_btn).align_right(Fill));

    let top_bar = container(top_row).padding([10, 14]).width(Fill);

    // Title
    let title = text("Whisper Model Setup").size(18).color(TEXT_COLOR);
    let subtitle = text("Select a model to download, or browse for an existing one.")
        .size(12)
        .color(MUTED);

    // Model list
    let models = crate::model_downloader::WHISPER_MODELS;
    let mut model_list = column![].spacing(4);
    for (idx, model) in models.iter().enumerate() {
        let is_selected = idx == state.wizard_selected_model;
        let label = text(format!(
            "{}  —  {}  —  {}",
            model.name, model.size_label, model.description
        ))
        .size(12)
        .color(if is_selected { PINK } else { TEXT_COLOR });

        let model_btn = button(label)
            .style(if is_selected {
                carousel_chip_active
            } else {
                carousel_chip_inactive
            })
            .padding([8, 12])
            .width(Fill)
            .on_press(Message::WizardSelectModel(idx));
        model_list = model_list.push(model_btn);
    }

    // Download / Cancel button
    let download_section = if state.wizard_downloading {
        let (downloaded, total) = state.wizard_download_progress.unwrap_or((0, 0));
        let pct = if total > 0 {
            (downloaded as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        let progress = progress_bar(0.0..=100.0, pct).height(8);
        let progress_text = if total > 0 {
            text(format!(
                "{:.1} MB / {:.1} MB ({:.0}%)",
                downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0,
                pct
            ))
            .size(11)
            .color(MUTED)
        } else {
            text("Downloading...").size(11).color(MUTED)
        };

        let cancel_btn = button(text("Cancel").size(13))
            .style(ghost_btn)
            .padding([8, 20])
            .on_press(Message::WizardCancelDownload);

        column![progress, progress_text, cancel_btn]
            .spacing(8)
            .align_x(iced::Alignment::Center)
    } else {
        let download_btn = button(text("Download").size(13))
            .style(primary_btn)
            .padding([8, 20])
            .on_press(Message::WizardStartDownload);

        column![download_btn].align_x(iced::Alignment::Center)
    };

    // Divider text
    let or_text = text("— or —").size(12).color(MUTED);

    // Browse section
    let browse_btn = button(
        row![
            icon('\u{E2C8}', 16.0),
            text("Browse for existing model").size(13)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .style(ghost_btn)
    .padding([8, 16])
    .on_press_maybe((!state.wizard_downloading).then_some(Message::WizardBrowseModel));

    // Error message
    let error_row: Element<'_, Message> = if let Some(ref err) = state.wizard_error {
        text(err).size(12).color(RED).into()
    } else {
        column![].into()
    };

    let body = column![
        title,
        subtitle,
        container(scrollable(model_list).height(200))
            .style(surface_container)
            .padding(8),
        download_section,
        container(or_text).center_x(Fill),
        container(browse_btn).center_x(Fill),
        error_row,
    ]
    .spacing(12)
    .padding([0, 20]);

    let content = column![top_bar, body];

    container(content)
        .style(bg_container)
        .width(Fill)
        .height(Fill)
        .into()
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat: implement wizard view rendering with model list and download UI"
```

---

### Task 7: Wire wizard message handlers and controller forwarding

**Files:**
- Modify: `src/ui.rs` (replace stub match arms with real logic)
- Modify: `src/controller.rs` (add match arms for download events)

This task replaces ALL stub match arms from Tasks 3 and 5 with real handlers, and wires the controller to forward download events.

- [ ] **Step 1: Replace wizard message stubs in `update()` with real handlers**

Replace the stub match arms added in Task 5 with:

```rust
Message::WizardSelectModel(idx) => {
    if idx < crate::model_downloader::WHISPER_MODELS.len() {
        state.wizard_selected_model = idx;
    }
    Task::none()
}
Message::WizardStartDownload => {
    if state.wizard_downloading {
        return Task::none();
    }
    state.wizard_downloading = true;
    state.wizard_download_progress = Some((0, 0));
    state.wizard_error = None;
    state.wizard_cancel_flag = Arc::new(AtomicBool::new(false));

    let model = &crate::model_downloader::WHISPER_MODELS[state.wizard_selected_model];
    crate::model_downloader::start_download(
        model,
        state.app_event_tx.clone(),
        Arc::clone(&state.wizard_cancel_flag),
    );
    Task::none()
}
Message::WizardCancelDownload => {
    state.wizard_cancel_flag.store(true, Ordering::Relaxed);
    Task::none()
}
Message::WizardBrowseModel => Task::perform(
    async {
        let handle = rfd::AsyncFileDialog::new()
            .set_title("Select Whisper Model")
            .add_filter("GGML Model", &["bin"])
            .pick_file()
            .await;
        handle.map(|h| h.path().to_string_lossy().into_owned())
    },
    Message::WizardModelPicked,
),
Message::WizardModelPicked(path) => {
    if let Some(path) = path {
        // Send the model path to the controller. The controller will
        // update config, restart the transcriber, and send a config
        // snapshot back. If the model fails to load, the transcriber
        // error will surface through the normal error path.
        state.send_event(AppEventKind::UiUpdateTranscriber(TranscriberConfig {
            model_path: path,
            ..state.snapshot_transcriber.clone().unwrap_or_default()
        }));
        state.phase = AppPhase::Main;
        state.wizard_error = None;
    }
    Task::none()
}
Message::WizardDownloadProgress(downloaded, total) => {
    state.wizard_download_progress = Some((downloaded, total));
    Task::none()
}
Message::WizardDownloadComplete(path) => {
    state.wizard_downloading = false;
    state.wizard_download_progress = None;
    state.phase = AppPhase::Main;
    state.wizard_error = None;
    // The controller already saved config and restarted the transcriber
    // when it received ModelDownloadComplete. We just transition the UI.
    let _ = path; // path already handled by controller
    Task::none()
}
Message::WizardDownloadFailed(err) => {
    state.wizard_downloading = false;
    state.wizard_download_progress = None;
    state.wizard_error = Some(err);
    Task::none()
}
Message::WizardDownloadCancelled => {
    state.wizard_downloading = false;
    state.wizard_download_progress = None;
    state.wizard_error = None;
    Task::none()
}
Message::WizardBack => {
    if state.wizard_from_settings {
        state.phase = AppPhase::Main;
        state.wizard_error = None;
        state.wizard_from_settings = false;
    }
    Task::none()
}
Message::OpenWizardFromSettings => {
    if state.mode != AppMode::Idle {
        return Task::none();
    }
    state.config_open = false;
    state.wizard_from_settings = true;
    state.wizard_downloading = false;
    state.wizard_download_progress = None;
    state.wizard_error = None;
    state.phase = AppPhase::Setup;
    Task::none()
}
```

- [ ] **Step 2: Replace `UiUpdate` stubs with real forwarding**

In the `Message::UiUpdateReceived(update)` match, replace the stubs from Task 3 with:

```rust
UiUpdate::ModelDownloadProgress(downloaded, total) => {
    return update(state, Message::WizardDownloadProgress(downloaded, total));
}
UiUpdate::ModelDownloadComplete(path) => {
    return update(state, Message::WizardDownloadComplete(path));
}
UiUpdate::ModelDownloadFailed(err) => {
    return update(state, Message::WizardDownloadFailed(err));
}
UiUpdate::ModelDownloadCancelled => {
    return update(state, Message::WizardDownloadCancelled);
}
```

- [ ] **Step 3: Add controller match arms for download events**

In `src/controller.rs`, add these match arms in the `match (event.source, event.kind)` block, BEFORE the catch-all `(source, kind) => { ... }` arm:

```rust
(_, AppEventKind::ModelDownloadProgress(downloaded, total)) => {
    let _ = self
        .ui_update_tx
        .send(UiUpdate::ModelDownloadProgress(downloaded, total));
}
(_, AppEventKind::ModelDownloadComplete(path)) => {
    info!("Model download complete: {}", path.display());
    let path_str = path.display().to_string();
    // Update config and restart transcriber. This is the ONLY place
    // where config is saved for downloads — the UI just transitions phase.
    self.app_state.update_transcriber(TranscriberConfig {
        model_path: path_str,
        ..self.app_state.transcriber_config()
    });
    self.restart_transcriber(self.app_state.transcriber_config());
    self.send_config_snapshot();
    let _ = self.ui_update_tx.send(UiUpdate::ModelDownloadComplete(path));
}
(_, AppEventKind::ModelDownloadFailed(err)) => {
    error!("Model download failed: {err}");
    let _ = self.ui_update_tx.send(UiUpdate::ModelDownloadFailed(err));
}
(_, AppEventKind::ModelDownloadCancelled) => {
    info!("Model download cancelled");
    let _ = self.ui_update_tx.send(UiUpdate::ModelDownloadCancelled);
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Build succeeds.

- [ ] **Step 5: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs src/controller.rs
git commit -m "feat: wire wizard message handlers and controller download forwarding"
```

---

### Task 8: Update settings UI — replace model path input with "Change Model" button

**Files:**
- Modify: `src/ui.rs` (update `view_setup_tab()`, remove old model path editing messages)

- [ ] **Step 1: Replace model path input with read-only display and "Change Model" button**

In `view_setup_tab()`, replace the model path input + browse button section (the `model_path_input`, `browse_btn`, and `model_path_row` variables around lines 1548-1561) with:

```rust
let model_display = text(&sf.model_path).size(12).color(MUTED);

// swap_horiz: E8D4
let change_model_btn = button(
    row![icon('\u{E8D4}', 16.0), text("Change Model").size(13)]
        .spacing(6)
        .align_y(iced::Alignment::Center),
)
.style(ghost_btn)
.padding([6, 14])
.on_press(Message::OpenWizardFromSettings);

let model_section = column![
    text("Model").size(11).color(MUTED),
    model_display,
    change_model_btn,
]
.spacing(4);
```

Update the transcriber card to use `model_section` instead of the old `model_path_row`:
```rust
let transcriber_card = column![
    text("Transcriber").size(15).color(TEXT_COLOR),
    model_section,
    // ... window_secs, overlap_secs, silence_thresh fields unchanged
]
```

- [ ] **Step 2: Remove old model path editing messages**

Remove from the `Message` enum:
- `ModelPathChanged(String)`
- `BrowseModelPath`
- `ModelPathPicked(Option<String>)`

Remove their match arms in `update()`.

The `model_path` field stays in `SetupFields` — it's used for read-only display. The `config_model_path` field stays in `UiRuntime` — it's used by `SaveConfig` to persist the current path.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Build succeeds.

- [ ] **Step 4: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "feat: replace model path input in settings with Change Model button"
```

---

### Task 9: Manual integration testing and polish

**Files:**
- Potentially modify: `src/ui.rs`, `src/config.rs`, `src/controller.rs` (bug fixes from testing)

This is a manual testing task. Run the app and verify each flow:

- [ ] **Step 1: Test first-launch flow**

1. Remove or rename `~/.config/arai/config.yaml` and any model files in the data directory
2. Run: `cargo run`
3. Verify: wizard appears (not main UI)
4. Select "Tiny (English)" and click Download
5. Verify: progress bar updates, download completes
6. Verify: app transitions to main UI
7. Verify: `~/.config/arai/config.yaml` has the new model path

- [ ] **Step 2: Test browse flow**

1. Remove model path from config or delete the model file
2. Run: `cargo run`
3. Click "Browse for existing model"
4. Pick a valid `.bin` model file
5. Verify: app transitions to main UI

- [ ] **Step 3: Test cancel download flow**

1. Start a download in the wizard
2. Click Cancel
3. Verify: download stops, `.part` file is cleaned up
4. Verify: wizard stays on screen, can retry

- [ ] **Step 4: Test "Change Model" from settings**

1. With a working model, open Settings
2. Click "Change Model"
3. Verify: wizard appears with Back button
4. Click Back
5. Verify: returns to main UI without changing model

- [ ] **Step 5: Test "Change Model" with new download**

1. Open Settings → "Change Model"
2. Download a different model
3. Verify: app returns to main UI with new model
4. Verify: transcription still works

- [ ] **Step 6: Fix any bugs found during testing**

Address issues as they come up. Common things to watch for:
- Window sizing — wizard content fits in 480x620
- Theme consistency — all wizard elements use Tokyo Night colors
- Error states — network failure shows meaningful message
- State cleanup — wizard state resets properly on transitions

- [ ] **Step 7: Final clippy and fmt pass**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 8: Commit any fixes**

```bash
git add -u
git commit -m "fix: polish wizard UI and fix integration issues"
```
