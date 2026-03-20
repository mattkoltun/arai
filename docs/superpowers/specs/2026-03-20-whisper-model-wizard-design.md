# Whisper Model Setup Wizard

## Overview

On first launch (or when the configured model path is invalid), the app displays a blocking setup wizard instead of the main UI. The wizard lets users either download a Whisper GGML model from Hugging Face or browse to an existing model file. The wizard is also re-accessible from settings.

## Default Model Path Change

The default model storage location changes from `models/ggml-small.en.bin` (relative to the binary) to the platform data directory resolved via `dirs::data_local_dir()`:
- macOS: `~/Library/Application Support/arai/models/<model-name>.bin`
- Linux: `~/.local/share/arai/models/<model-name>.bin`

The `DEFAULT_MODEL_PATH` constant in `config.rs` updates accordingly. Existing users with a custom `model_path` in their config are unaffected.

## First-Launch Config Bootstrap

Currently, `Config::load()` fails fatally if no API key is configured, which prevents the wizard from ever appearing on a truly fresh install. To fix this:

- Make `open_api_key` optional at load time (allow empty/missing). `Config::load()` succeeds even without an API key.
- Add a `Config::is_ready() -> bool` method that checks if the API key is present and the model path is valid.
- The wizard phase only requires a valid model path. The API key check moves to the transition into `AppPhase::Main` — if no API key is set, the main UI can show a prompt in settings (existing behavior once the user reaches the main app).

This means `from_partial()` in `config.rs` no longer returns `Err(ConfigError::MissingApiKey)` when the key is empty. Instead, `open_api_key` defaults to an empty string, and the agent module checks for it at call time (it already handles this — `agent.rs` is called on submit, not at startup).

## App Phase State Machine

A new enum in `messages.rs` controls what the UI renders:

```rust
enum AppPhase {
    Setup(SetupState),
    Main(MainState),
}
```

**Transition logic on launch:**
1. Load config (succeeds even without API key or valid model path)
2. Check if `config.transcriber.model_path` points to an existing file
3. If yes: enter `AppPhase::Main`, start transcriber normally
4. If no: enter `AppPhase::Setup`, show wizard

**Transition from Setup to Main:**
Once a model is successfully downloaded or selected, save the path to config via `Config::save()`, initialize the transcriber, and transition to `AppPhase::Main`.

**Transition from Main to Setup (settings):**
The settings panel gets a "Change Model" button. This button is only enabled when `AppMode::Idle` (no recording or processing in progress). Clicking it transitions back to `AppPhase::Setup`. The existing model path text input and browse button in settings are replaced by this "Change Model" button to avoid a confusing dual-path UI. When a new model is selected, the transcriber restarts with the new model path (existing restart logic in `controller.rs` handles this).

**Cancel/Back from Setup (when accessed from settings):**
When the wizard is entered from settings (not first launch), a "Cancel" button is shown that returns to `AppPhase::Main` without changing the model. On first launch, there is no cancel — the wizard is blocking.

## Wizard UI

Single screen, Tokyo Night themed to match the main app. Fits within the existing 480x620 window. Two sections:

### Section 1: Download a Model

A selectable list of Whisper model variants:

| Model | File | Size | Description |
|-------|------|------|-------------|
| Tiny (English) | `ggml-tiny.en.bin` | ~75 MB | Fastest, least accurate |
| Base (English) | `ggml-base.en.bin` | ~142 MB | Fast, decent accuracy |
| Small (English) | `ggml-small.en.bin` | ~487 MB | Good balance (recommended) |
| Medium (English) | `ggml-medium.en.bin` | ~1.5 GB | High accuracy, slower |
| Large | `ggml-large-v3-turbo.bin` | ~1.5 GB | Best accuracy, multilingual |

User selects a model, clicks "Download". A progress bar appears showing download progress. The download button is disabled during download.

Download URL pattern: `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-<model>.bin`

Download destination: platform data directory (created via `std::fs::create_dir_all` if it doesn't exist).

### Section 2: Use an Existing Model

A "Browse..." button opens a native file dialog (`.bin` filter). When a file is selected, validation runs on a background thread (loading a Whisper model is heavyweight — hundreds of MB, GPU context init). A spinner/loading indicator is shown during validation. If valid, proceeds. If invalid, shows an error.

### Completion

On success (download complete or valid file selected):
1. Update `config.transcriber.model_path` to the new path (stored as `String` in `TranscriberConfig`, converted from `PathBuf` via `.display().to_string()`)
2. Call `Config::save()`
3. Transition to `AppPhase::Main`

## Download Implementation

- Use `reqwest::blocking` with manual chunked reads in a loop for progress tracking. This avoids needing a separate tokio runtime on the download thread, consistent with the codebase's synchronous threading style.
- Download runs on a background thread (spawned via `std::thread`)
- Progress updates sent to the UI via the existing `AppEvent` channel (new event kind: `ModelDownloadProgress(u64, u64)` for bytes received / total bytes)
- A new event kind `ModelDownloadComplete(PathBuf)` signals success
- A new event kind `ModelDownloadFailed(String)` signals failure
- The file is written to a `.part` temp file during download, then renamed via `std::fs::rename` on completion (atomic on same filesystem)
- Cancel uses an `Arc<AtomicBool>` flag (consistent with transcriber's `stop_flag` pattern). The download loop checks this flag each chunk. On cancel, the `.part` file is deleted.

## Error Handling

- Network failure during download: show error message with "Retry" button
- Invalid model file selected via browse: show error, stay on wizard
- Disk full / write failure: show error message
- No internet available: download button shows error, browse still works

## Known Limitations

- Downloads are not resumable. If a large download fails partway, it restarts from scratch. Resumable downloads (HTTP Range headers) can be added later.
- No checksum verification of downloaded models. Corruption is possible but unlikely over HTTPS. Can be added later with SHA256 checks against Hugging Face metadata.

## Changes to Existing Code

### `config.rs`
- Change `DEFAULT_MODEL_PATH` to use `dirs::data_local_dir()` + `arai/models/ggml-small.en.bin`
- Add `pub fn default_model_dir() -> PathBuf` returning the platform data directory + `arai/models/`
- Remove the `MissingApiKey` error from `from_partial()`. Allow empty API key at load time.

### `main.rs`
- Remove the early exit when transcriber fails to start due to missing model
- Pass model-existence info to the UI so it can choose the initial phase

### `ui.rs`
- Add `AppPhase`-aware rendering: wizard view vs main view
- Add setup state struct with model selection, download progress, cancel flag, etc.
- Add wizard view function rendering the setup screen
- Replace model path input + browse button in settings with a "Change Model" button (enabled only when idle)
- Wire download events to progress bar updates

### `messages.rs`
- Add new `AppEventKind` variants: `ModelDownloadProgress`, `ModelDownloadComplete`, `ModelDownloadFailed`
- Add `AppPhase` enum

### `controller.rs`
- Handle new model download events, forwarding progress to UI
- On `ModelDownloadComplete`, update config and signal UI to transition

## New Dependencies

- `dirs` — cross-platform standard directories (`data_local_dir()`)

Note: `reqwest` is already a dependency (with `blocking` and `json` features). No new HTTP dependency needed — just use `reqwest::blocking` with chunked reads. `futures` is already a dependency. `iced` already pulls in `tokio`.

## Testing

- Unit test: `default_model_dir()` returns expected path structure
- Unit test: config loads successfully without API key (no longer fatal)
- Manual test: first launch with no model triggers wizard
- Manual test: download completes and transitions to main UI
- Manual test: browse selects valid model and transitions
- Manual test: settings "Change Model" button returns to wizard
- Manual test: cancel download cleans up `.part` file
- Manual test: cancel/back from wizard when accessed from settings returns to main UI
